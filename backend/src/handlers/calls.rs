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
pub struct OpenCall {
    pub share_lot_id: i64,
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
pub struct CloseCall {
    pub action: String,
    pub close_date: Option<String>,
    pub close_premium: Option<f64>,
    pub fees_close: Option<f64>,
}

pub async fn list_share_lots(
    State(pool): State<SqlitePool>,
    Path(account_id): Path<i64>,
) -> Result<Json<Vec<ShareLot>>, AppError> {
    ShareLot::list_active(&pool, account_id).await.map(Json)
}

pub async fn open_call(
    State(pool): State<SqlitePool>,
    Path(account_id): Path<i64>,
    Json(payload): Json<OpenCall>,
) -> Result<(StatusCode, Json<Trade>), AppError> {
    // Verify the lot belongs to this account and is ACTIVE
    let lot = ShareLot::get(&pool, payload.share_lot_id).await?;
    if lot.account_id != account_id {
        return Err(AppError::BadRequest(
            "Share lot does not belong to this account".to_string(),
        ));
    }
    if lot.status != "ACTIVE" {
        return Err(AppError::BadRequest("Share lot is not ACTIVE".to_string()));
    }

    let input = CreateTrade {
        account_id,
        trade_type: "CALL".to_string(),
        ticker: payload.ticker,
        strike_price: payload.strike_price,
        expiry_date: payload.expiry_date,
        open_date: payload.open_date,
        premium_received: payload.premium_received,
        fees_open: payload.fees_open,
        share_lot_id: Some(payload.share_lot_id),
        quantity: payload.quantity,
        rolled_from_trade_id: payload.rolled_from_trade_id,
    };
    let trade = Trade::create(&pool, &input).await?;

    if let Some(prev_id) = payload.rolled_from_trade_id {
        Trade::set_rolled_to(&pool, prev_id, trade.id).await?;
        // The previous call was already closed as BOUGHT_BACK, which may have spiked
        // the lot's cost basis by a large debit. Now that the roll link is established,
        // recalculate so the debit is deferred until the new call settles — reflecting
        // the blended net of both legs to the user immediately.
        let prev_trade = Trade::get(&pool, prev_id).await?;
        if let Some(lot_id) = prev_trade.share_lot_id {
            ShareLot::recalculate_cost_basis(&pool, lot_id).await?;
        }
    }

    Ok((StatusCode::CREATED, Json(trade)))
}

pub async fn close_call(
    State(pool): State<SqlitePool>,
    Path(trade_id): Path<i64>,
    Json(payload): Json<CloseCall>,
) -> Result<Json<serde_json::Value>, AppError> {
    let trade = Trade::get(&pool, trade_id).await?;

    if trade.trade_type != "CALL" {
        return Err(AppError::BadRequest("Trade is not a CALL".to_string()));
    }
    if trade.status != "OPEN" {
        return Err(AppError::BadRequest("Trade is not OPEN".to_string()));
    }

    let lot_id = trade
        .share_lot_id
        .ok_or_else(|| AppError::BadRequest("CALL trade has no share_lot_id".to_string()))?;

    match payload.action.as_str() {
        "EXPIRED" | "BOUGHT_BACK" => {
            let (status, close_premium) = if payload.action == "BOUGHT_BACK" {
                let cp = payload.close_premium.ok_or_else(|| {
                    AppError::BadRequest("close_premium required for BOUGHT_BACK".to_string())
                })?;
                ("BOUGHT_BACK", Some(cp))
            } else {
                ("EXPIRED", None)
            };

            let updated = Trade::close(
                &pool,
                trade_id,
                status,
                close_premium,
                payload.fees_close,
                payload.close_date,
            )
            .await?;

            // Use full recalculation (not incremental reduce_cost_basis) so that the
            // roll-aware logic in recalculate_cost_basis can correctly handle the case
            // where this BOUGHT_BACK call is the "from" leg of a roll: once the new
            // call's rolled_from link is established (in open_call), recalculate will
            // exclude this call until the rolled-to call settles.
            let lot = ShareLot::recalculate_cost_basis(&pool, lot_id).await?;

            Ok(Json(json!({
                "trade": updated,
                "share_lot": lot
            })))
        }
        "CALLED_AWAY" => {
            let updated = Trade::close(
                &pool,
                trade_id,
                "CALLED_AWAY",
                payload.close_premium,
                payload.fees_close,
                payload.close_date,
            )
            .await?;

            ShareLot::recalculate_cost_basis(&pool, lot_id).await?;
            ShareLot::mark_called_away(&pool, lot_id).await?;

            Ok(Json(json!(updated)))
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

    /// Creates account -> PUT -> assigns PUT -> returns (server, account_id, lot_id)
    async fn server_with_lot() -> (TestServer, i64, i64) {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;

        let s = TestServer::new(create_router(pool.clone())).unwrap();

        // Create account
        let res = s.post("/api/accounts").json(&json!({"name": "Test"})).await;
        let acct_id = res.json::<serde_json::Value>()["id"].as_i64().unwrap();

        // Open PUT
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

        // Assign PUT to create share lot
        let res = s
            .post(&format!("/api/trades/puts/{}/close", trade_id))
            .json(&json!({
                "action": "ASSIGNED",
                "close_date": "2025-02-21"
            }))
            .await;
        let body = res.json::<serde_json::Value>();
        let lot_id = body["share_lot"]["id"].as_i64().unwrap();

        // Create a new server from the same pool for test isolation
        let s2 = TestServer::new(create_router(pool)).unwrap();
        (s2, acct_id, lot_id)
    }

    #[tokio::test]
    async fn test_open_call_on_lot() {
        let (server, acct_id, lot_id) = server_with_lot().await;

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
        res.assert_status(StatusCode::CREATED);
        let body = res.json::<serde_json::Value>();
        assert_eq!(body["status"], "OPEN");
        assert_eq!(body["trade_type"], "CALL");
        assert_eq!(body["share_lot_id"], lot_id);
    }

    #[tokio::test]
    async fn test_close_call_expired_reduces_cost_basis() {
        let (server, acct_id, lot_id) = server_with_lot().await;

        // Check initial adjusted cost basis of the lot
        let lots_res = server
            .get(&format!("/api/accounts/{}/share-lots", acct_id))
            .await;
        let lots = lots_res.json::<serde_json::Value>();
        let initial_cb = lots.as_array().unwrap()[0]["adjusted_cost_basis"]
            .as_f64()
            .unwrap();

        // Open a CALL
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

        // Close as EXPIRED
        let res = server
            .post(&format!("/api/trades/calls/{}/close", call_id))
            .json(&json!({
                "action": "EXPIRED",
                "close_date": "2025-03-21"
            }))
            .await;
        res.assert_status(StatusCode::OK);
        let body = res.json::<serde_json::Value>();

        let new_cb = body["share_lot"]["adjusted_cost_basis"].as_f64().unwrap();
        // net_premium = 150 - 1.30 = 148.70, reduction = 148.70 / 100 = 1.487
        let expected_reduction = 1.487;
        assert!((initial_cb - new_cb - expected_reduction).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_close_call_called_away_marks_lot() {
        let (server, acct_id, lot_id) = server_with_lot().await;

        // Open a CALL
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

        // Close as CALLED_AWAY
        let res = server
            .post(&format!("/api/trades/calls/{}/close", call_id))
            .json(&json!({
                "action": "CALLED_AWAY",
                "close_date": "2025-03-21"
            }))
            .await;
        res.assert_status(StatusCode::OK);

        // Verify active lots is empty
        let lots_res = server
            .get(&format!("/api/accounts/{}/share-lots", acct_id))
            .await;
        let lots = lots_res.json::<serde_json::Value>();
        assert_eq!(lots.as_array().unwrap().len(), 0);
    }

    /// Regression: rolling a call at a debit must not spike cost basis until the
    /// new (rolled-to) call settles. Cost basis should reflect the blended net.
    #[tokio::test]
    async fn test_roll_defers_cost_basis_until_settled() {
        let (server, acct_id, lot_id) = server_with_lot().await;

        // Get baseline cost basis (after PUT assignment)
        let lots_res = server
            .get(&format!("/api/accounts/{}/share-lots", acct_id))
            .await;
        let baseline_cb = lots_res.json::<serde_json::Value>().as_array().unwrap()[0]
            ["adjusted_cost_basis"]
            .as_f64()
            .unwrap();

        // Open the original call: sold for $150
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
        let old_call_id = res.json::<serde_json::Value>()["id"].as_i64().unwrap();

        // Close it as BOUGHT_BACK at a big debit ($500) to simulate a debit roll
        let res = server
            .post(&format!("/api/trades/calls/{}/close", old_call_id))
            .json(&json!({
                "action": "BOUGHT_BACK",
                "close_date": "2025-03-20",
                "close_premium": 500.0,
                "fees_close": 1.30
            }))
            .await;
        res.assert_status(StatusCode::OK);
        // At this point the roll link is not established yet — cost basis will reflect the debit
        // (this is expected interim state before the new call is opened)

        // Open the new call (the rolled-to leg): sold for $600
        let res = server
            .post(&format!("/api/accounts/{}/calls", acct_id))
            .json(&json!({
                "share_lot_id": lot_id,
                "ticker": "AAPL",
                "strike_price": 160.0,
                "expiry_date": "2025-04-18",
                "open_date": "2025-03-20",
                "premium_received": 600.0,
                "fees_open": 1.30,
                "rolled_from_trade_id": old_call_id
            }))
            .await;
        res.assert_status(StatusCode::CREATED);
        let new_call_id = res.json::<serde_json::Value>()["id"].as_i64().unwrap();

        // Now the roll is linked. Cost basis should be back to baseline (old call deferred).
        let lots_res = server
            .get(&format!("/api/accounts/{}/share-lots", acct_id))
            .await;
        let cb_mid_roll = lots_res.json::<serde_json::Value>().as_array().unwrap()[0]
            ["adjusted_cost_basis"]
            .as_f64()
            .unwrap();
        assert!(
            (cb_mid_roll - baseline_cb).abs() < 0.001,
            "mid-roll cost basis should equal baseline ({:.4}), got {:.4}",
            baseline_cb,
            cb_mid_roll
        );

        // Close the new call as EXPIRED — full blended net now applies:
        // old net = 150 - 1.30 - 500 - 1.30 = -352.60, per_share = -3.526
        // new net = 600 - 1.30 = 598.70, per_share = 5.987
        // combined per_share = 2.461 reduction
        let res = server
            .post(&format!("/api/trades/calls/{}/close", new_call_id))
            .json(&json!({
                "action": "EXPIRED",
                "close_date": "2025-04-18"
            }))
            .await;
        res.assert_status(StatusCode::OK);
        let cb_after_roll = res.json::<serde_json::Value>()["share_lot"]["adjusted_cost_basis"]
            .as_f64()
            .unwrap();

        let expected = baseline_cb - ((-352.60 + 598.70) / 100.0);
        assert!(
            (cb_after_roll - expected).abs() < 0.001,
            "post-roll cost basis should be {:.4}, got {:.4}",
            expected,
            cb_after_roll
        );
    }
}
