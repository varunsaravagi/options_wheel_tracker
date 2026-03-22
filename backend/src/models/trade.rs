use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use crate::errors::AppError;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Trade {
    pub id: i64,
    pub account_id: i64,
    pub trade_type: String,
    pub ticker: String,
    pub strike_price: f64,
    pub expiry_date: String,
    pub open_date: String,
    pub premium_received: f64,
    pub fees_open: f64,
    pub status: String,
    pub close_date: Option<String>,
    pub close_premium: Option<f64>,
    pub fees_close: Option<f64>,
    pub share_lot_id: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateTrade {
    pub account_id: i64,
    pub trade_type: String,
    pub ticker: String,
    pub strike_price: f64,
    pub expiry_date: String,
    pub open_date: String,
    pub premium_received: f64,
    pub fees_open: f64,
    pub share_lot_id: Option<i64>,
}

impl Trade {
    pub fn net_premium(&self) -> Option<f64> {
        Some(
            self.premium_received
                - self.fees_open
                - self.close_premium.unwrap_or(0.0)
                - self.fees_close.unwrap_or(0.0),
        )
    }

    pub async fn create(pool: &SqlitePool, input: &CreateTrade) -> Result<Trade, AppError> {
        let trade = sqlx::query_as::<_, Trade>(
            "INSERT INTO trades (account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, share_lot_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             RETURNING id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, created_at"
        )
        .bind(input.account_id)
        .bind(&input.trade_type)
        .bind(&input.ticker)
        .bind(input.strike_price)
        .bind(&input.expiry_date)
        .bind(&input.open_date)
        .bind(input.premium_received)
        .bind(input.fees_open)
        .bind(input.share_lot_id)
        .fetch_one(pool)
        .await?;
        Ok(trade)
    }

    pub async fn get(pool: &SqlitePool, id: i64) -> Result<Trade, AppError> {
        let trade = sqlx::query_as::<_, Trade>(
            "SELECT id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, created_at
             FROM trades WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        trade.ok_or(AppError::NotFound)
    }

    pub async fn close(
        pool: &SqlitePool,
        id: i64,
        status: &str,
        close_premium: Option<f64>,
        fees_close: Option<f64>,
        close_date: Option<String>,
    ) -> Result<Trade, AppError> {
        let date = close_date.unwrap_or_else(|| {
            chrono::Local::now().format("%Y-%m-%d").to_string()
        });

        let result = sqlx::query(
            "UPDATE trades SET status = ?, close_premium = ?, fees_close = ?, close_date = ? WHERE id = ?"
        )
        .bind(status)
        .bind(close_premium)
        .bind(fees_close)
        .bind(&date)
        .bind(id)
        .execute(pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }

        Self::get(pool, id).await
    }

    pub async fn list_open(pool: &SqlitePool, account_id: i64) -> Result<Vec<Trade>, AppError> {
        let trades = sqlx::query_as::<_, Trade>(
            "SELECT id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, created_at
             FROM trades WHERE account_id = ? AND status = 'OPEN'"
        )
        .bind(account_id)
        .fetch_all(pool)
        .await?;
        Ok(trades)
    }

    pub async fn list_with_filters(
        pool: &SqlitePool,
        account_id: Option<i64>,
        ticker: Option<&str>,
        date_from: Option<&str>,
        date_to: Option<&str>,
    ) -> Result<Vec<Trade>, AppError> {
        let all = sqlx::query_as::<_, Trade>(
            "SELECT id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, created_at
             FROM trades ORDER BY open_date DESC"
        )
        .fetch_all(pool)
        .await?;

        let filtered = all
            .into_iter()
            .filter(|t| account_id.map_or(true, |aid| t.account_id == aid))
            .filter(|t| ticker.map_or(true, |tk| t.ticker == tk))
            .filter(|t| date_from.map_or(true, |df| t.open_date.as_str() >= df))
            .filter(|t| date_to.map_or(true, |dt| t.open_date.as_str() <= dt))
            .collect();

        Ok(filtered)
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
    async fn test_create_put_trade() {
        let (pool, account_id) = setup().await;

        let input = CreateTrade {
            account_id,
            trade_type: "PUT".to_string(),
            ticker: "AAPL".to_string(),
            strike_price: 150.0,
            expiry_date: "2025-02-21".to_string(),
            open_date: "2025-01-15".to_string(),
            premium_received: 200.0,
            fees_open: 1.30,
            share_lot_id: None,
        };

        let trade = Trade::create(&pool, &input).await.unwrap();
        assert_eq!(trade.status, "OPEN");
        assert_eq!(trade.trade_type, "PUT");
    }

    #[tokio::test]
    async fn test_close_trade_expired() {
        let (pool, account_id) = setup().await;

        let input = CreateTrade {
            account_id,
            trade_type: "PUT".to_string(),
            ticker: "AAPL".to_string(),
            strike_price: 150.0,
            expiry_date: "2025-02-21".to_string(),
            open_date: "2025-01-15".to_string(),
            premium_received: 200.0,
            fees_open: 1.30,
            share_lot_id: None,
        };

        let trade = Trade::create(&pool, &input).await.unwrap();
        let closed = Trade::close(&pool, trade.id, "EXPIRED", None, None, Some("2025-02-21".to_string()))
            .await
            .unwrap();
        assert_eq!(closed.status, "EXPIRED");
    }

    #[tokio::test]
    async fn test_net_premium_expired() {
        let (pool, account_id) = setup().await;

        let input = CreateTrade {
            account_id,
            trade_type: "PUT".to_string(),
            ticker: "AAPL".to_string(),
            strike_price: 150.0,
            expiry_date: "2025-02-21".to_string(),
            open_date: "2025-01-15".to_string(),
            premium_received: 200.0,
            fees_open: 1.30,
            share_lot_id: None,
        };

        let trade = Trade::create(&pool, &input).await.unwrap();
        let closed = Trade::close(&pool, trade.id, "EXPIRED", None, None, Some("2025-02-21".to_string()))
            .await
            .unwrap();

        let net = closed.net_premium().unwrap();
        assert!((net - 198.70).abs() < 0.001);
    }
}
