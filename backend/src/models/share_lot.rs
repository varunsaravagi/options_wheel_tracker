use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use crate::errors::AppError;

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
        let adjusted = input.adjusted_cost_basis.unwrap_or(input.original_cost_basis);
        let lot = sqlx::query_as::<_, ShareLot>(
            "INSERT INTO share_lots (account_id, ticker, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             RETURNING id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id, status, created_at"
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
            "SELECT id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id, status, created_at
             FROM share_lots WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        lot.ok_or(AppError::NotFound)
    }

    pub async fn list_active(pool: &SqlitePool, account_id: i64) -> Result<Vec<ShareLot>, AppError> {
        let lots = sqlx::query_as::<_, ShareLot>(
            "SELECT id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id, status, created_at
             FROM share_lots WHERE account_id = ? AND status = 'ACTIVE' ORDER BY acquisition_date DESC"
        )
        .bind(account_id)
        .fetch_all(pool)
        .await?;
        Ok(lots)
    }

    pub async fn reduce_cost_basis(pool: &SqlitePool, id: i64, premium_total: f64) -> Result<(), AppError> {
        let lot = Self::get(pool, id).await?;
        let per_share = premium_total / lot.quantity as f64;
        let result = sqlx::query(
            "UPDATE share_lots SET adjusted_cost_basis = adjusted_cost_basis - ? WHERE id = ?"
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
        let result = sqlx::query(
            "UPDATE share_lots SET status = 'CALLED_AWAY' WHERE id = ?"
        )
        .bind(id)
        .execute(pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
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
        ShareLot::reduce_cost_basis(&pool, lot.id, 50.0).await.unwrap();

        let updated = ShareLot::get(&pool, lot.id).await.unwrap();
        assert!((updated.adjusted_cost_basis - 149.50).abs() < 0.001);
    }
}
