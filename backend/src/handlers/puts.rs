use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;

use crate::{
    errors::AppError,
    models::{
        share_lot::ShareLot,
        trade::{CreateTrade, Trade},
    },
};

#[derive(Deserialize)]
pub struct OpenPut {
    pub ticker: String,
    pub strike_price: f64,
    pub expiry_date: String,
    pub open_date: String,
    pub premium_received: f64,
    pub fees_open: f64,
    pub quantity: Option<i64>,
}

#[derive(Deserialize)]
pub struct ClosePut {
    pub action: String,
    pub close_date: Option<String>,
    pub close_premium: Option<f64>,
    pub fees_close: Option<f64>,
}

pub async fn open_put(
    State(pool): State<SqlitePool>,
    Path(account_id): Path<i64>,
    Json(payload): Json<OpenPut>,
) -> Result<(StatusCode, Json<Trade>), AppError> {
    let input = CreateTrade {
        account_id,
        trade_type: "PUT".to_string(),
        ticker: payload.ticker,
        strike_price: payload.strike_price,
        expiry_date: payload.expiry_date,
        open_date: payload.open_date,
        premium_received: payload.premium_received,
        fees_open: payload.fees_open,
        share_lot_id: None,
        quantity: payload.quantity,
    };
    let trade = Trade::create(&pool, &input).await?;
    Ok((StatusCode::CREATED, Json(trade)))
}

pub async fn close_put(
    State(pool): State<SqlitePool>,
    Path(trade_id): Path<i64>,
    Json(payload): Json<ClosePut>,
) -> Result<Json<serde_json::Value>, AppError> {
    let trade = Trade::get(&pool, trade_id).await?;

    if trade.trade_type != "PUT" {
        return Err(AppError::BadRequest("Trade is not a PUT".to_string()));
    }
    if trade.status != "OPEN" {
        return Err(AppError::BadRequest("Trade is not OPEN".to_string()));
    }

    match payload.action.as_str() {
        "EXPIRED" => {
            let updated =
                Trade::close(&pool, trade_id, "EXPIRED", None, None, payload.close_date).await?;
            Ok(Json(json!(updated)))
        }
        "BOUGHT_BACK" => {
            let close_premium = payload.close_premium.ok_or_else(|| {
                AppError::BadRequest("close_premium required for BOUGHT_BACK".to_string())
            })?;
            let updated = Trade::close(
                &pool,
                trade_id,
                "BOUGHT_BACK",
                Some(close_premium),
                payload.fees_close,
                payload.close_date,
            )
            .await?;
            Ok(Json(json!(updated)))
        }
        "ASSIGNED" => {
            let close_date = payload
                .close_date
                .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());

            let mut tx = pool.begin().await.map_err(AppError::Database)?;

            // Close the trade within the transaction
            sqlx::query(
                "UPDATE trades SET status = 'ASSIGNED', close_date = ?, close_premium = ?, fees_close = ? WHERE id = ?"
            )
            .bind(&close_date)
            .bind(payload.close_premium)
            .bind(payload.fees_close)
            .bind(trade_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::Database)?;

            // Calculate adjusted cost basis
            let net_per_share =
                (trade.premium_received - trade.fees_open) / (100.0 * trade.quantity as f64);
            let adjusted_cb = trade.strike_price - net_per_share;
            let lot_quantity = 100 * trade.quantity;

            // Create share lot within the transaction
            let lot = sqlx::query_as::<_, ShareLot>(
                "INSERT INTO share_lots (account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id)
                 VALUES (?, ?, ?, ?, ?, ?, 'ASSIGNED', ?)
                 RETURNING id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id, status, sale_price, sale_date, created_at"
            )
            .bind(trade.account_id)
            .bind(&trade.ticker)
            .bind(lot_quantity)
            .bind(trade.strike_price)
            .bind(adjusted_cb)
            .bind(&close_date)
            .bind(trade_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(AppError::Database)?;

            tx.commit().await.map_err(AppError::Database)?;

            // Re-fetch the updated trade
            let updated = Trade::get(&pool, trade_id).await?;

            Ok(Json(json!({
                "trade": updated,
                "share_lot": lot
            })))
        }
        _ => Err(AppError::BadRequest(format!(
            "Invalid action: {}",
            payload.action
        ))),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use axum_test::TestServer;
    use serde_json::json;

    use crate::db;
    use crate::routes::create_router;

    async fn server() -> (TestServer, i64) {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let app = create_router(pool.clone());
        let s = TestServer::new(app).unwrap();
        let res = s.post("/api/accounts").json(&json!({"name": "Test"})).await;
        let id = res.json::<serde_json::Value>()["id"].as_i64().unwrap();
        let s2 = TestServer::new(create_router(pool)).unwrap();
        (s2, id)
    }

    #[tokio::test]
    async fn test_open_put() {
        let (server, acct_id) = server().await;
        let res = server
            .post(&format!("/api/accounts/{}/puts", acct_id))
            .json(&json!({
                "ticker": "AAPL",
                "strike_price": 150.0,
                "expiry_date": "2025-02-21",
                "open_date": "2025-01-15",
                "premium_received": 200.0,
                "fees_open": 1.30
            }))
            .await;
        res.assert_status(StatusCode::CREATED);
        let body = res.json::<serde_json::Value>();
        assert_eq!(body["status"], "OPEN");
        assert_eq!(body["trade_type"], "PUT");
    }

    #[tokio::test]
    async fn test_close_put_expired() {
        let (server, acct_id) = server().await;
        let create_res = server
            .post(&format!("/api/accounts/{}/puts", acct_id))
            .json(&json!({
                "ticker": "AAPL",
                "strike_price": 150.0,
                "expiry_date": "2025-02-21",
                "open_date": "2025-01-15",
                "premium_received": 200.0,
                "fees_open": 1.30
            }))
            .await;
        let trade_id = create_res.json::<serde_json::Value>()["id"]
            .as_i64()
            .unwrap();

        let res = server
            .post(&format!("/api/trades/puts/{}/close", trade_id))
            .json(&json!({
                "action": "EXPIRED",
                "close_date": "2025-02-21"
            }))
            .await;
        res.assert_status(StatusCode::OK);
        let body = res.json::<serde_json::Value>();
        assert_eq!(body["status"], "EXPIRED");
    }

    #[tokio::test]
    async fn test_close_put_assigned_creates_lot() {
        let (server, acct_id) = server().await;
        let create_res = server
            .post(&format!("/api/accounts/{}/puts", acct_id))
            .json(&json!({
                "ticker": "AAPL",
                "strike_price": 150.0,
                "expiry_date": "2025-02-21",
                "open_date": "2025-01-15",
                "premium_received": 200.0,
                "fees_open": 1.30
            }))
            .await;
        let trade_id = create_res.json::<serde_json::Value>()["id"]
            .as_i64()
            .unwrap();

        let res = server
            .post(&format!("/api/trades/puts/{}/close", trade_id))
            .json(&json!({
                "action": "ASSIGNED",
                "close_date": "2025-02-21"
            }))
            .await;
        res.assert_status(StatusCode::OK);
        let body = res.json::<serde_json::Value>();
        assert_eq!(body["trade"]["status"], "ASSIGNED");

        let lot = &body["share_lot"];
        assert!(lot["id"].is_number());
        // adjusted_cb = 150 - (200 - 1.30) / 100 = 150 - 1.987 = 148.013
        let adjusted = lot["adjusted_cost_basis"].as_f64().unwrap();
        assert!((adjusted - 148.013).abs() < 0.001);
    }
}
