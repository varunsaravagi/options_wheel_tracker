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
        trade::{CreateTrade, Trade, UpdateTrade},
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
    pub rolled_from_trade_id: Option<i64>,
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
        rolled_from_trade_id: payload.rolled_from_trade_id,
    };
    let trade = Trade::create(&pool, &input).await?;

    if let Some(prev_id) = payload.rolled_from_trade_id {
        Trade::set_rolled_to(&pool, prev_id, trade.id).await?;
    }

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

pub async fn edit_trade(
    State(pool): State<SqlitePool>,
    Path(trade_id): Path<i64>,
    Json(payload): Json<UpdateTrade>,
) -> Result<Json<serde_json::Value>, AppError> {
    let existing = Trade::get(&pool, trade_id).await?;
    let updated = Trade::update(&pool, trade_id, &payload).await?;

    // If this is a CALL trade linked to a share lot, recalculate the lot's cost basis
    if updated.trade_type == "CALL" && updated.status != "OPEN" {
        if let Some(lot_id) = updated.share_lot_id {
            let lot = ShareLot::recalculate_cost_basis(&pool, lot_id).await?;
            return Ok(Json(json!({
                "trade": updated,
                "share_lot": lot
            })));
        }
    }

    // If this is an ASSIGNED PUT, recalculate the lot sourced from this trade
    if existing.trade_type == "PUT" && existing.status == "ASSIGNED" {
        // Find the share lot sourced from this PUT
        let lot = sqlx::query_as::<_, ShareLot>(
            "SELECT id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id, status, sale_price, sale_date, created_at
             FROM share_lots WHERE source_trade_id = ?"
        )
        .bind(trade_id)
        .fetch_optional(&pool)
        .await
        .map_err(AppError::Database)?;

        if let Some(lot) = lot {
            let recalculated = ShareLot::recalculate_cost_basis(&pool, lot.id).await?;
            return Ok(Json(json!({
                "trade": updated,
                "share_lot": recalculated
            })));
        }
    }

    Ok(Json(json!(updated)))
}

pub async fn delete_trade(
    State(pool): State<SqlitePool>,
    Path(trade_id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let existing = Trade::get(&pool, trade_id).await?;
    let deleted = Trade::soft_delete(&pool, trade_id).await?;

    // If this was a closed CALL trade linked to a share lot, recalculate cost basis
    if existing.trade_type == "CALL" && existing.status != "OPEN" {
        if let Some(lot_id) = existing.share_lot_id {
            let lot = ShareLot::recalculate_cost_basis(&pool, lot_id).await?;
            return Ok(Json(json!({
                "trade": deleted,
                "share_lot": lot
            })));
        }
    }

    Ok(Json(json!(deleted)))
}

#[derive(Deserialize)]
pub struct LinkRollPayload {
    pub target_trade_id: i64,
}

pub async fn link_roll(
    State(pool): State<SqlitePool>,
    Path(source_id): Path<i64>,
    Json(payload): Json<LinkRollPayload>,
) -> Result<Json<serde_json::Value>, AppError> {
    Trade::get(&pool, source_id).await?;
    Trade::get(&pool, payload.target_trade_id).await?;

    Trade::set_rolled_to(&pool, source_id, payload.target_trade_id).await?;
    Trade::set_rolled_from(&pool, payload.target_trade_id, source_id).await?;

    Ok(Json(json!({
        "source_id": source_id,
        "target_id": payload.target_trade_id
    })))
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

    #[tokio::test]
    async fn test_delete_trade() {
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

        let res = server.delete(&format!("/api/trades/{}", trade_id)).await;
        res.assert_status(StatusCode::OK);
        let body = res.json::<serde_json::Value>();
        assert!(body["deleted_at"].as_str().is_some());

        // Deleted trade should be excluded from dashboard calculations
        let dash = server
            .get(&format!("/api/dashboard?account_id={}", acct_id))
            .await;
        let dash_body = dash.json::<serde_json::Value>();
        assert_eq!(dash_body["total_premium_collected"].as_f64().unwrap(), 0.0);
        // But should still appear in open_trades list
        assert_eq!(dash_body["open_trades"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_edit_open_trade() {
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
            .put(&format!("/api/trades/{}", trade_id))
            .json(&json!({
                "premium_received": 250.0,
                "fees_open": 2.00,
                "strike_price": 155.0
            }))
            .await;
        res.assert_status(StatusCode::OK);
        let body = res.json::<serde_json::Value>();
        assert_eq!(body["premium_received"], 250.0);
        assert_eq!(body["fees_open"], 2.0);
        assert_eq!(body["strike_price"], 155.0);
        // Unchanged fields should remain
        assert_eq!(body["ticker"], "AAPL");
        assert_eq!(body["status"], "OPEN");
    }

    #[tokio::test]
    async fn test_edit_closed_trade() {
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

        // Close the trade first
        server
            .post(&format!("/api/trades/puts/{}/close", trade_id))
            .json(&json!({
                "action": "BOUGHT_BACK",
                "close_date": "2025-02-15",
                "close_premium": 50.0,
                "fees_close": 1.30
            }))
            .await;

        // Edit the closed trade
        let res = server
            .put(&format!("/api/trades/{}", trade_id))
            .json(&json!({
                "close_premium": 60.0,
                "fees_close": 2.00
            }))
            .await;
        res.assert_status(StatusCode::OK);
        let body = res.json::<serde_json::Value>();
        assert_eq!(body["close_premium"], 60.0);
        assert_eq!(body["fees_close"], 2.0);
        assert_eq!(body["status"], "BOUGHT_BACK");
    }

    /// Helper: creates account -> PUT -> ASSIGNED -> returns (server, account_id, lot_id)
    async fn server_with_lot() -> (TestServer, i64, i64) {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let s = TestServer::new(create_router(pool.clone())).unwrap();

        let res = s.post("/api/accounts").json(&json!({"name": "Test"})).await;
        let acct_id = res.json::<serde_json::Value>()["id"].as_i64().unwrap();

        let res = s
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
        let trade_id = res.json::<serde_json::Value>()["id"].as_i64().unwrap();

        let res = s
            .post(&format!("/api/trades/puts/{}/close", trade_id))
            .json(&json!({
                "action": "ASSIGNED",
                "close_date": "2025-02-21"
            }))
            .await;
        let lot_id = res.json::<serde_json::Value>()["share_lot"]["id"]
            .as_i64()
            .unwrap();

        let s2 = TestServer::new(create_router(pool)).unwrap();
        (s2, acct_id, lot_id)
    }

    #[tokio::test]
    async fn test_edit_closed_call_recalculates_cost_basis() {
        let (server, acct_id, lot_id) = server_with_lot().await;

        // Open and close a CALL
        let res = server
            .post(&format!("/api/accounts/{}/calls", acct_id))
            .json(&json!({
                "share_lot_id": lot_id,
                "ticker": "AAPL",
                "strike_price": 155.0,
                "expiry_date": "2025-03-21",
                "open_date": "2025-02-22",
                "premium_received": 150.0,
                "fees_open": 1.30
            }))
            .await;
        let call_id = res.json::<serde_json::Value>()["id"].as_i64().unwrap();

        server
            .post(&format!("/api/trades/calls/{}/close", call_id))
            .json(&json!({
                "action": "EXPIRED",
                "close_date": "2025-03-21"
            }))
            .await;

        // Get cost basis after close
        let lots_res = server
            .get(&format!("/api/accounts/{}/share-lots", acct_id))
            .await;
        let cb_after_close = lots_res.json::<serde_json::Value>().as_array().unwrap()[0]
            ["adjusted_cost_basis"]
            .as_f64()
            .unwrap();

        // Edit the closed CALL to increase premium
        let res = server
            .put(&format!("/api/trades/{}", call_id))
            .json(&json!({
                "premium_received": 300.0
            }))
            .await;
        res.assert_status(StatusCode::OK);
        let body = res.json::<serde_json::Value>();
        let new_cb = body["share_lot"]["adjusted_cost_basis"].as_f64().unwrap();

        // With higher premium (300 vs 150), cost basis should be lower
        assert!(new_cb < cb_after_close);

        // Verify exact: initial adjusted_cb = 150 - (200-1.30)/100 = 148.013
        // CALL net = 300 - 1.30 = 298.70, reduction = 298.70 / 100 = 2.987
        // expected = 148.013 - 2.987 = 145.026
        assert!((new_cb - 145.026).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_delete_closed_call_reverses_cost_basis() {
        let (server, acct_id, lot_id) = server_with_lot().await;

        // Get initial cost basis
        let lots_res = server
            .get(&format!("/api/accounts/{}/share-lots", acct_id))
            .await;
        let initial_cb = lots_res.json::<serde_json::Value>().as_array().unwrap()[0]
            ["adjusted_cost_basis"]
            .as_f64()
            .unwrap();

        // Open and close a CALL
        let res = server
            .post(&format!("/api/accounts/{}/calls", acct_id))
            .json(&json!({
                "share_lot_id": lot_id,
                "ticker": "AAPL",
                "strike_price": 155.0,
                "expiry_date": "2025-03-21",
                "open_date": "2025-02-22",
                "premium_received": 150.0,
                "fees_open": 1.30
            }))
            .await;
        let call_id = res.json::<serde_json::Value>()["id"].as_i64().unwrap();

        server
            .post(&format!("/api/trades/calls/{}/close", call_id))
            .json(&json!({
                "action": "EXPIRED",
                "close_date": "2025-03-21"
            }))
            .await;

        // Delete the CALL trade
        let res = server.delete(&format!("/api/trades/{}", call_id)).await;
        res.assert_status(StatusCode::OK);
        let body = res.json::<serde_json::Value>();
        let restored_cb = body["share_lot"]["adjusted_cost_basis"].as_f64().unwrap();

        // Cost basis should be restored to initial value (before CALL premium reduction)
        assert!((restored_cb - initial_cb).abs() < 0.001);
    }
}
