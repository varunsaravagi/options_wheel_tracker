use crate::errors::AppError;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct ShareLot {
    pub id: i64,
    pub account_id: i64,
    pub ticker: String,
    pub quantity: i64,
    pub original_cost_basis: f64,
    pub adjusted_cost_basis: f64,
    pub acquisition_date: String,
    pub acquisition_type: String,
    pub source_trade_id: Option<i64>,
    pub status: String,
    pub sale_price: Option<f64>,
    pub sale_date: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateShareLot {
    pub account_id: i64,
    pub ticker: String,
    pub original_cost_basis: f64,
    pub adjusted_cost_basis: Option<f64>,
    pub acquisition_date: String,
    pub acquisition_type: String,
    pub source_trade_id: Option<i64>,
}

impl ShareLot {
    pub async fn create(pool: &SqlitePool, input: &CreateShareLot) -> Result<ShareLot, AppError> {
        let adjusted = input
            .adjusted_cost_basis
            .unwrap_or(input.original_cost_basis);
        let lot = sqlx::query_as::<_, ShareLot>(
            "INSERT INTO share_lots (account_id, ticker, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             RETURNING id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id, status, sale_price, sale_date, created_at"
        )
        .bind(input.account_id)
        .bind(&input.ticker)
        .bind(input.original_cost_basis)
        .bind(adjusted)
        .bind(&input.acquisition_date)
        .bind(&input.acquisition_type)
        .bind(input.source_trade_id)
        .fetch_one(pool)
        .await?;
        Ok(lot)
    }

    pub async fn get(pool: &SqlitePool, id: i64) -> Result<ShareLot, AppError> {
        let lot = sqlx::query_as::<_, ShareLot>(
            "SELECT id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id, status, sale_price, sale_date, created_at
             FROM share_lots WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        lot.ok_or(AppError::NotFound)
    }

    pub async fn list_active(
        pool: &SqlitePool,
        account_id: i64,
    ) -> Result<Vec<ShareLot>, AppError> {
        let lots = sqlx::query_as::<_, ShareLot>(
            "SELECT id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id, status, sale_price, sale_date, created_at
             FROM share_lots WHERE account_id = ? AND status = 'ACTIVE' ORDER BY acquisition_date DESC"
        )
        .bind(account_id)
        .fetch_all(pool)
        .await?;
        Ok(lots)
    }

    pub async fn reduce_cost_basis(
        pool: &SqlitePool,
        id: i64,
        premium_total: f64,
    ) -> Result<(), AppError> {
        let lot = Self::get(pool, id).await?;
        let per_share = premium_total / lot.quantity as f64;
        let result = sqlx::query(
            "UPDATE share_lots SET adjusted_cost_basis = adjusted_cost_basis - ? WHERE id = ?",
        )
        .bind(per_share)
        .bind(id)
        .execute(pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub async fn mark_called_away(pool: &SqlitePool, id: i64) -> Result<(), AppError> {
        let result = sqlx::query("UPDATE share_lots SET status = 'CALLED_AWAY' WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    /// Recalculate adjusted_cost_basis for a share lot from scratch.
    /// Computes initial adjusted CB from the source PUT trade (if ASSIGNED),
    /// then subtracts net_premium/lot.quantity for each closed, non-deleted CALL trade.
    pub async fn recalculate_cost_basis(pool: &SqlitePool, id: i64) -> Result<ShareLot, AppError> {
        let lot = Self::get(pool, id).await?;

        // Start from the original cost basis (strike price for ASSIGNED, user-entered for MANUAL)
        let mut adjusted_cb = lot.original_cost_basis;

        // For ASSIGNED lots, subtract the source PUT's premium per share
        if lot.acquisition_type == "ASSIGNED" {
            if let Some(source_id) = lot.source_trade_id {
                let source_trade = sqlx::query_as::<_, crate::models::trade::Trade>(
                    "SELECT id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, quantity, created_at, deleted_at
                     FROM trades WHERE id = ? AND deleted_at IS NULL"
                )
                .bind(source_id)
                .fetch_optional(pool)
                .await?;

                if let Some(put_trade) = source_trade {
                    let net_per_share = (put_trade.premium_received - put_trade.fees_open)
                        / (100.0 * put_trade.quantity as f64);
                    adjusted_cb -= net_per_share;
                }
            }
        }

        // Subtract net premium per share for each closed, non-deleted CALL trade on this lot
        let call_trades = sqlx::query_as::<_, crate::models::trade::Trade>(
            "SELECT id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, quantity, created_at, deleted_at
             FROM trades WHERE share_lot_id = ? AND trade_type = 'CALL' AND status != 'OPEN' AND deleted_at IS NULL"
        )
        .bind(id)
        .fetch_all(pool)
        .await?;

        for call in &call_trades {
            let net = call.net_premium().unwrap_or(0.0);
            adjusted_cb -= net / lot.quantity as f64;
        }

        let result = sqlx::query_as::<_, ShareLot>(
            "UPDATE share_lots SET adjusted_cost_basis = ? WHERE id = ?
             RETURNING id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id, status, sale_price, sale_date, created_at"
        )
        .bind(adjusted_cb)
        .bind(id)
        .fetch_one(pool)
        .await?;

        Ok(result)
    }

    /// Recalculate adjusted_cost_basis for all share lots in the database.
    pub async fn recalculate_all_cost_bases(pool: &SqlitePool) -> Result<Vec<ShareLot>, AppError> {
        let lot_ids: Vec<(i64,)> = sqlx::query_as("SELECT id FROM share_lots")
            .fetch_all(pool)
            .await?;

        let mut results = Vec::new();
        for (lot_id,) in lot_ids {
            let lot = Self::recalculate_cost_basis(pool, lot_id).await?;
            results.push(lot);
        }
        Ok(results)
    }

    pub async fn mark_sold(
        pool: &SqlitePool,
        id: i64,
        sale_price: f64,
        sale_date: &str,
    ) -> Result<ShareLot, AppError> {
        let lot = sqlx::query_as::<_, ShareLot>(
            "UPDATE share_lots SET status = 'SOLD', sale_price = ?, sale_date = ?
             WHERE id = ? AND status = 'ACTIVE'
             RETURNING id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id, status, sale_price, sale_date, created_at"
        )
        .bind(sale_price)
        .bind(sale_date)
        .bind(id)
        .fetch_optional(pool)
        .await?;
        lot.ok_or(AppError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::models::account::Account;

    async fn setup() -> (crate::db::Pool, i64) {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let acct = Account::create(&pool, "Test").await.unwrap();
        (pool, acct.id)
    }

    #[tokio::test]
    async fn test_create_and_list_lot() {
        let (pool, account_id) = setup().await;

        let input = CreateShareLot {
            account_id,
            ticker: "AAPL".to_string(),
            original_cost_basis: 150.00,
            adjusted_cost_basis: None,
            acquisition_date: "2025-01-15".to_string(),
            acquisition_type: "MANUAL".to_string(),
            source_trade_id: None,
        };

        let lot = ShareLot::create(&pool, &input).await.unwrap();
        assert_eq!(lot.original_cost_basis, 150.00);
        assert_eq!(lot.adjusted_cost_basis, 150.00);
        assert_eq!(lot.status, "ACTIVE");

        let lots = ShareLot::list_active(&pool, account_id).await.unwrap();
        assert_eq!(lots.len(), 1);
    }

    #[tokio::test]
    async fn test_reduce_cost_basis() {
        let (pool, account_id) = setup().await;

        let input = CreateShareLot {
            account_id,
            ticker: "AAPL".to_string(),
            original_cost_basis: 150.00,
            adjusted_cost_basis: None,
            acquisition_date: "2025-01-15".to_string(),
            acquisition_type: "MANUAL".to_string(),
            source_trade_id: None,
        };

        let lot = ShareLot::create(&pool, &input).await.unwrap();
        ShareLot::reduce_cost_basis(&pool, lot.id, 50.0)
            .await
            .unwrap();

        let updated = ShareLot::get(&pool, lot.id).await.unwrap();
        assert!((updated.adjusted_cost_basis - 149.50).abs() < 0.001);
    }
}
