use crate::errors::AppError;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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
    pub quantity: i64,
    pub created_at: String,
    pub deleted_at: Option<String>,
    pub rolled_from_trade_id: Option<i64>,
    pub rolled_to_trade_id: Option<i64>,
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
    pub quantity: Option<i64>,
    pub rolled_from_trade_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTrade {
    pub strike_price: Option<f64>,
    pub expiry_date: Option<String>,
    pub open_date: Option<String>,
    pub premium_received: Option<f64>,
    pub fees_open: Option<f64>,
    pub quantity: Option<i64>,
    pub close_date: Option<String>,
    pub close_premium: Option<f64>,
    pub fees_close: Option<f64>,
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
        let qty = input.quantity.unwrap_or(1);
        let trade = sqlx::query_as::<_, Trade>(
            "INSERT INTO trades (account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, share_lot_id, quantity, rolled_from_trade_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             RETURNING id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, quantity, created_at, deleted_at, rolled_from_trade_id, rolled_to_trade_id"
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
        .bind(qty)
        .bind(input.rolled_from_trade_id)
        .fetch_one(pool)
        .await?;
        Ok(trade)
    }

    pub async fn get(pool: &SqlitePool, id: i64) -> Result<Trade, AppError> {
        let trade = sqlx::query_as::<_, Trade>(
            "SELECT id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, quantity, created_at, deleted_at, rolled_from_trade_id, rolled_to_trade_id
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
        let date =
            close_date.unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());

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

    pub async fn update(
        pool: &SqlitePool,
        id: i64,
        input: &UpdateTrade,
    ) -> Result<Trade, AppError> {
        let existing = Self::get(pool, id).await?;

        let strike_price = input.strike_price.unwrap_or(existing.strike_price);
        let expiry_date = input
            .expiry_date
            .clone()
            .unwrap_or(existing.expiry_date.clone());
        let open_date = input
            .open_date
            .clone()
            .unwrap_or(existing.open_date.clone());
        let premium_received = input.premium_received.unwrap_or(existing.premium_received);
        let fees_open = input.fees_open.unwrap_or(existing.fees_open);
        let quantity = input.quantity.unwrap_or(existing.quantity);

        // For closed trades, allow editing close fields
        let close_date = if existing.status != "OPEN" {
            input.close_date.clone().or(existing.close_date.clone())
        } else {
            existing.close_date.clone()
        };
        let close_premium = if existing.status != "OPEN" {
            input.close_premium.or(existing.close_premium)
        } else {
            existing.close_premium
        };
        let fees_close = if existing.status != "OPEN" {
            input.fees_close.or(existing.fees_close)
        } else {
            existing.fees_close
        };

        let result = sqlx::query(
            "UPDATE trades SET strike_price = ?, expiry_date = ?, open_date = ?, premium_received = ?, fees_open = ?, quantity = ?, close_date = ?, close_premium = ?, fees_close = ? WHERE id = ?",
        )
        .bind(strike_price)
        .bind(&expiry_date)
        .bind(&open_date)
        .bind(premium_received)
        .bind(fees_open)
        .bind(quantity)
        .bind(&close_date)
        .bind(close_premium)
        .bind(fees_close)
        .bind(id)
        .execute(pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }

        Self::get(pool, id).await
    }

    pub async fn soft_delete(pool: &SqlitePool, id: i64) -> Result<Trade, AppError> {
        let existing = Self::get(pool, id).await?;
        if existing.deleted_at.is_some() {
            return Err(AppError::BadRequest("Trade is already deleted".to_string()));
        }

        let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let result = sqlx::query("UPDATE trades SET deleted_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id)
            .execute(pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }

        Self::get(pool, id).await
    }

    pub async fn set_rolled_to(
        pool: &SqlitePool,
        id: i64,
        rolled_to_id: i64,
    ) -> Result<(), AppError> {
        let result = sqlx::query("UPDATE trades SET rolled_to_trade_id = ? WHERE id = ?")
            .bind(rolled_to_id)
            .bind(id)
            .execute(pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub async fn set_rolled_from(
        pool: &SqlitePool,
        id: i64,
        rolled_from_id: i64,
    ) -> Result<(), AppError> {
        let result = sqlx::query("UPDATE trades SET rolled_from_trade_id = ? WHERE id = ?")
            .bind(rolled_from_id)
            .bind(id)
            .execute(pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }

    pub async fn list_open(pool: &SqlitePool, account_id: i64) -> Result<Vec<Trade>, AppError> {
        let trades = sqlx::query_as::<_, Trade>(
            "SELECT id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, quantity, created_at, deleted_at, rolled_from_trade_id, rolled_to_trade_id
             FROM trades WHERE account_id = ? AND status = 'OPEN' AND deleted_at IS NULL"
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
            "SELECT id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, quantity, created_at, deleted_at, rolled_from_trade_id, rolled_to_trade_id
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
            quantity: None,
            rolled_from_trade_id: None,
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
            quantity: None,
            rolled_from_trade_id: None,
        };

        let trade = Trade::create(&pool, &input).await.unwrap();
        let closed = Trade::close(
            &pool,
            trade.id,
            "EXPIRED",
            None,
            None,
            Some("2025-02-21".to_string()),
        )
        .await
        .unwrap();
        assert_eq!(closed.status, "EXPIRED");
    }

    #[tokio::test]
    async fn test_soft_delete() {
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
            quantity: None,
            rolled_from_trade_id: None,
        };

        let trade = Trade::create(&pool, &input).await.unwrap();
        assert!(trade.deleted_at.is_none());

        let deleted = Trade::soft_delete(&pool, trade.id).await.unwrap();
        assert!(deleted.deleted_at.is_some());

        // Soft-deleted trade should not appear in list_open
        let open = Trade::list_open(&pool, account_id).await.unwrap();
        assert!(open.is_empty());

        // But should still appear in list_with_filters
        let all = Trade::list_with_filters(&pool, Some(account_id), None, None, None)
            .await
            .unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].deleted_at.is_some());

        // Deleting again should fail
        let err = Trade::soft_delete(&pool, trade.id).await;
        assert!(err.is_err());
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
            quantity: None,
            rolled_from_trade_id: None,
        };

        let trade = Trade::create(&pool, &input).await.unwrap();
        let closed = Trade::close(
            &pool,
            trade.id,
            "EXPIRED",
            None,
            None,
            Some("2025-02-21".to_string()),
        )
        .await
        .unwrap();

        let net = closed.net_premium().unwrap();
        assert!((net - 198.70).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_roll_linking() {
        let (pool, account_id) = setup().await;

        let input = CreateTrade {
            account_id,
            trade_type: "PUT".to_string(),
            ticker: "AAPL".to_string(),
            strike_price: 150.0,
            expiry_date: "2025-02-21".to_string(),
            open_date: "2025-01-10".to_string(),
            premium_received: 200.0,
            fees_open: 1.3,
            share_lot_id: None,
            quantity: None,
            rolled_from_trade_id: None,
        };
        let trade_a = Trade::create(&pool, &input).await.unwrap();

        let input_b = CreateTrade {
            open_date: "2025-01-20".to_string(),
            rolled_from_trade_id: Some(trade_a.id),
            ..input
        };
        let trade_b = Trade::create(&pool, &input_b).await.unwrap();

        Trade::set_rolled_to(&pool, trade_a.id, trade_b.id)
            .await
            .unwrap();

        let a = Trade::get(&pool, trade_a.id).await.unwrap();
        let b = Trade::get(&pool, trade_b.id).await.unwrap();

        assert_eq!(a.rolled_to_trade_id, Some(trade_b.id));
        assert_eq!(b.rolled_from_trade_id, Some(trade_a.id));
    }
}
