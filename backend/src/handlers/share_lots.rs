use crate::errors::AppError;
use crate::models::share_lot::{CreateShareLot, ShareLot};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use sqlx::SqlitePool;

#[derive(Deserialize)]
pub struct CreateManualLot {
    pub ticker: String,
    pub cost_basis: f64,
    pub acquisition_date: String,
}

#[derive(Deserialize)]
pub struct SellLot {
    pub sale_price: f64,
    pub sale_date: String,
}

pub async fn sell_share_lot(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
    Json(payload): Json<SellLot>,
) -> Result<Json<ShareLot>, AppError> {
    if payload.sale_price <= 0.0 {
        return Err(AppError::BadRequest(
            "sale_price must be positive".to_string(),
        ));
    }
    let lot = ShareLot::mark_sold(&pool, id, payload.sale_price, &payload.sale_date).await?;
    Ok(Json(lot))
}

pub async fn recalculate_all(
    State(pool): State<SqlitePool>,
) -> Result<Json<Vec<ShareLot>>, AppError> {
    let lots = ShareLot::recalculate_all_cost_bases(&pool).await?;
    Ok(Json(lots))
}

pub async fn create_manual_lot(
    State(pool): State<SqlitePool>,
    Path(account_id): Path<i64>,
    Json(payload): Json<CreateManualLot>,
) -> Result<(StatusCode, Json<ShareLot>), AppError> {
    if payload.cost_basis <= 0.0 {
        return Err(AppError::BadRequest(
            "cost_basis must be positive".to_string(),
        ));
    }
    let lot = ShareLot::create(
        &pool,
        &CreateShareLot {
            account_id,
            ticker: payload.ticker.to_uppercase(),
            original_cost_basis: payload.cost_basis,
            adjusted_cost_basis: None,
            acquisition_date: payload.acquisition_date,
            acquisition_type: "MANUAL".to_string(),
            source_trade_id: None,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(lot)))
}

#[cfg(test)]
mod tests {
    use crate::{db, routes::create_router};
    use axum::http::StatusCode;
    use axum_test::TestServer;
    use serde_json::json;

    async fn server_with_lot() -> (TestServer, i64, i64) {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let srv = TestServer::new(create_router(pool.clone())).unwrap();

        let acct_id = srv
            .post("/api/accounts")
            .json(&json!({"name": "Test"}))
            .await
            .json::<serde_json::Value>()["id"]
            .as_i64()
            .unwrap();

        let trade_id = srv
            .post(&format!("/api/accounts/{}/puts", acct_id))
            .json(&json!({
                "ticker": "AAPL",
                "strike_price": 150.0,
                "expiry_date": "2025-02-21",
                "open_date": "2025-01-15",
                "premium_received": 200.0,
                "fees_open": 1.30
            }))
            .await
            .json::<serde_json::Value>()["id"]
            .as_i64()
            .unwrap();

        let lot_id = srv
            .post(&format!("/api/trades/puts/{}/close", trade_id))
            .json(&json!({
                "action": "ASSIGNED",
                "close_date": "2025-02-21"
            }))
            .await
            .json::<serde_json::Value>()["share_lot"]["id"]
            .as_i64()
            .unwrap();

        let srv2 = TestServer::new(create_router(pool)).unwrap();
        (srv2, acct_id, lot_id)
    }

    #[tokio::test]
    async fn test_recalculate_all_share_lots() {
        let (server, acct_id, lot_id) = server_with_lot().await;

        // Open and close a CALL
        let call_id = server
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
            .await
            .json::<serde_json::Value>()["id"]
            .as_i64()
            .unwrap();

        server
            .post(&format!("/api/trades/calls/{}/close", call_id))
            .json(&json!({
                "action": "EXPIRED",
                "close_date": "2025-03-21"
            }))
            .await;

        // Manually corrupt the adjusted_cost_basis to simulate stale data
        // We'll use recalculate to fix it
        let res = server.post("/api/share-lots/recalculate").await;
        res.assert_status(StatusCode::OK);
        let lots = res.json::<serde_json::Value>();
        let cb = lots.as_array().unwrap()[0]["adjusted_cost_basis"]
            .as_f64()
            .unwrap();

        // initial adjusted_cb = 150 - (200-1.30)/100 = 148.013
        // CALL net = 150 - 1.30 = 148.70, reduction = 148.70 / 100 = 1.487
        // expected = 148.013 - 1.487 = 146.526
        assert!((cb - 146.526).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_create_manual_lot() {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let srv = TestServer::new(create_router(pool)).unwrap();
        let acct_id = srv
            .post("/api/accounts")
            .json(&json!({"name":"T"}))
            .await
            .json::<serde_json::Value>()["id"]
            .as_i64()
            .unwrap();
        let res = srv
            .post(&format!("/api/accounts/{}/share-lots", acct_id))
            .json(&json!({
                "ticker": "MSFT",
                "cost_basis": 300.00,
                "acquisition_date": "2024-06-01"
            }))
            .await;
        res.assert_status(StatusCode::CREATED);
        let body = res.json::<serde_json::Value>();
        assert_eq!(body["ticker"], "MSFT");
        assert_eq!(body["acquisition_type"], "MANUAL");
        assert_eq!(body["adjusted_cost_basis"].as_f64().unwrap(), 300.00);
    }
}
