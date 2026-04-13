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

    /// Returns true if the terminal trade at the end of a roll chain (following
    /// rolled_to_trade_id links) is still OPEN. Used to defer cost basis changes
    /// for in-progress rolls until the final leg settles.
    async fn roll_chain_terminal_is_open(
        pool: &SqlitePool,
        trade_id: i64,
    ) -> Result<bool, AppError> {
        let mut current_id = trade_id;
        loop {
            let row: Option<(String, Option<i64>)> = sqlx::query_as(
                "SELECT status, rolled_to_trade_id FROM trades WHERE id = ? AND deleted_at IS NULL",
            )
            .bind(current_id)
            .fetch_optional(pool)
            .await?;

            match row {
                None => return Ok(false),
                Some((_, Some(next_id))) => current_id = next_id,
                Some((status, None)) => return Ok(status == "OPEN"),
            }
        }
    }

    /// Recalculate adjusted_cost_basis for a share lot from scratch.
    /// Computes initial adjusted CB from the source PUT trade (if ASSIGNED),
    /// then subtracts net_premium/lot.quantity for each closed, non-deleted CALL trade.
    ///
    /// Roll-aware: a BOUGHT_BACK call that is the "from" leg of an in-progress roll
    /// is skipped until the terminal call in the chain settles. This ensures the full
    /// blended net of the roll is applied at once rather than the debit leg appearing
    /// immediately while the credit leg is deferred.
    pub async fn recalculate_cost_basis(pool: &SqlitePool, id: i64) -> Result<ShareLot, AppError> {
        let lot = Self::get(pool, id).await?;

        // Start from the original cost basis (strike price for ASSIGNED, user-entered for MANUAL)
        let mut adjusted_cb = lot.original_cost_basis;

        // For ASSIGNED lots, subtract the source PUT's premium per share
        if lot.acquisition_type == "ASSIGNED" {
            if let Some(source_id) = lot.source_trade_id {
                let source_trade = sqlx::query_as::<_, crate::models::trade::Trade>(
                    "SELECT id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, quantity, created_at, deleted_at, rolled_from_trade_id, rolled_to_trade_id
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

        // Subtract net premium per share for each closed, non-deleted CALL trade on this lot.
        // Skip calls that are the "from" leg of an in-progress roll (their rolled_to is still
        // OPEN) — the debit is deferred until the terminal call settles so the full roll net
        // is reflected as one blended reduction.
        let call_trades = sqlx::query_as::<_, crate::models::trade::Trade>(
            "SELECT id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, quantity, created_at, deleted_at, rolled_from_trade_id, rolled_to_trade_id
             FROM trades WHERE share_lot_id = ? AND trade_type = 'CALL' AND status != 'OPEN' AND deleted_at IS NULL"
        )
        .bind(id)
        .fetch_all(pool)
        .await?;

        for call in &call_trades {
            if let Some(rolled_to_id) = call.rolled_to_trade_id {
                if Self::roll_chain_terminal_is_open(pool, rolled_to_id).await? {
                    continue;
                }
            }
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
    async fn test_recalculate_defers_roll_in_progress() {
        // Regression: rolling a call at a debit should not spike cost basis until
        // the new (rolled-to) call settles.
        use crate::models::trade::{CreateTrade, Trade};

        let (pool, account_id) = setup().await;

        // Create share lot manually — original_cost_basis is the starting point for
        // recalculate_cost_basis on MANUAL lots (no PUT trade to look up).
        let lot = ShareLot::create(
            &pool,
            &CreateShareLot {
                account_id,
                ticker: "TEST".to_string(),
                original_cost_basis: 100.0,
                adjusted_cost_basis: None,
                acquisition_date: "2025-01-01".to_string(),
                acquisition_type: "MANUAL".to_string(),
                source_trade_id: None,
            },
        )
        .await
        .unwrap();

        // Create old call: sold for $200, bought back for $500 (debit roll)
        let old_call = Trade::create(
            &pool,
            &CreateTrade {
                account_id,
                trade_type: "CALL".to_string(),
                ticker: "TEST".to_string(),
                strike_price: 105.0,
                expiry_date: "2025-02-21".to_string(),
                open_date: "2025-01-15".to_string(),
                premium_received: 200.0,
                fees_open: 0.66,
                share_lot_id: Some(lot.id),
                quantity: Some(1),
                rolled_from_trade_id: None,
            },
        )
        .await
        .unwrap();

        // Close old call as BOUGHT_BACK (debit: net = 200 - 0.66 - 500 - 0.66 = -301.32)
        Trade::close(
            &pool,
            old_call.id,
            "BOUGHT_BACK",
            Some(500.0),
            Some(0.66),
            Some("2025-02-20".to_string()),
        )
        .await
        .unwrap();

        // Create new call (the rolled-to leg): sold for $600
        let new_call = Trade::create(
            &pool,
            &CreateTrade {
                account_id,
                trade_type: "CALL".to_string(),
                ticker: "TEST".to_string(),
                strike_price: 110.0,
                expiry_date: "2025-03-21".to_string(),
                open_date: "2025-02-20".to_string(),
                premium_received: 600.0,
                fees_open: 0.66,
                share_lot_id: Some(lot.id),
                quantity: Some(1),
                rolled_from_trade_id: Some(old_call.id),
            },
        )
        .await
        .unwrap();

        // Link the roll
        Trade::set_rolled_to(&pool, old_call.id, new_call.id)
            .await
            .unwrap();

        // While new call is OPEN: old call should be excluded — cost basis stays at
        // original_cost_basis (100.0) with no settled calls contributing.
        let recalculated = ShareLot::recalculate_cost_basis(&pool, lot.id)
            .await
            .unwrap();
        assert!(
            (recalculated.adjusted_cost_basis - 100.0).abs() < 0.001,
            "expected 100.0 while roll is in progress, got {}",
            recalculated.adjusted_cost_basis
        );

        // Close new call as EXPIRED (net = 600 - 0.66 = 599.34)
        Trade::close(
            &pool,
            new_call.id,
            "EXPIRED",
            None,
            None,
            Some("2025-03-21".to_string()),
        )
        .await
        .unwrap();

        // Now both legs are closed. Combined net of the roll:
        //   old leg: 200 - 0.66 - 500 - 0.66 = -301.32
        //   new leg: 600 - 0.66        = 599.34
        //   combined per_share = (-301.32 + 599.34) / 100 = 2.9802
        //   adjusted_cb = 100.0 - 2.9802 = 97.0198
        let recalculated = ShareLot::recalculate_cost_basis(&pool, lot.id)
            .await
            .unwrap();
        let expected = 100.0 - ((-301.32 + 599.34) / 100.0);
        assert!(
            (recalculated.adjusted_cost_basis - expected).abs() < 0.001,
            "expected {:.4} after roll settles, got {}",
            expected,
            recalculated.adjusted_cost_basis
        );
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
