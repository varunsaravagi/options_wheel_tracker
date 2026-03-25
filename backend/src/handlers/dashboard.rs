use crate::errors::AppError;
use crate::handlers::yield_calc::{calculate_yields, get_capital_for_trade, round2};
use crate::models::share_lot::ShareLot;
use crate::models::trade::Trade;
use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Deserialize)]
pub struct DashboardQuery {
    pub account_id: Option<i64>,
}

#[derive(Serialize)]
pub struct DashboardResponse {
    pub total_premium_collected: f64,
    pub total_capital_deployed: f64,
    pub realized_annualized_yield: f64,
    pub open_annualized_yield: f64,
    pub open_trades: Vec<Trade>,
    pub active_share_lots: Vec<ShareLot>,
}

pub async fn get_dashboard(
    State(pool): State<SqlitePool>,
    Query(params): Query<DashboardQuery>,
) -> Result<Json<DashboardResponse>, AppError> {
    let trades = Trade::list_with_filters(&pool, params.account_id, None, None, None).await?;

    let yields = calculate_yields(&pool, &trades).await;

    let mut total_premium = 0.0;
    let mut open_trades = Vec::new();
    let mut total_capital_deployed = 0.0;

    for trade in &trades {
        let capital = get_capital_for_trade(&pool, trade).await;

        if trade.status == "OPEN" {
            open_trades.push(trade.clone());
            total_capital_deployed += capital;
        } else {
            total_premium += trade.net_premium().unwrap_or(0.0);
        }
    }

    let realized_annualized_yield = yields.realized_yield;
    let open_annualized_yield = yields.open_yield;

    // Fetch active share lots
    let active_share_lots = if let Some(account_id) = params.account_id {
        ShareLot::list_active(&pool, account_id).await?
    } else {
        // No account_id filter: fetch all active lots via runtime query
        sqlx::query_as::<_, ShareLot>(
            "SELECT id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id, status, sale_price, sale_date, created_at
             FROM share_lots WHERE status = 'ACTIVE' ORDER BY acquisition_date DESC"
        )
        .fetch_all(&pool)
        .await?
    };

    // Capital in active share lots (assigned puts or manual entries) is still deployed.
    // Skip lots that have an open CALL against them — that capital is already counted
    // via get_capital_for_trade for the CALL trade.
    let open_call_lot_ids: Vec<i64> = open_trades
        .iter()
        .filter(|t| t.trade_type == "CALL")
        .filter_map(|t| t.share_lot_id)
        .collect();
    for lot in &active_share_lots {
        if !open_call_lot_ids.contains(&lot.id) {
            total_capital_deployed += lot.adjusted_cost_basis * lot.quantity as f64;
        }
    }

    // Round monetary values to 2 decimal places
    let total_premium = round2(total_premium);
    let total_capital_deployed = round2(total_capital_deployed);

    Ok(Json(DashboardResponse {
        total_premium_collected: total_premium,
        total_capital_deployed,
        realized_annualized_yield,
        open_annualized_yield,
        open_trades,
        active_share_lots,
    }))
}

#[cfg(test)]
mod tests {
    use crate::{db, routes::create_router};
    use axum_test::TestServer;
    use serde_json::json;

    #[tokio::test]
    async fn test_dashboard_totals() {
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
        srv.post(&format!("/api/accounts/{}/puts", acct_id))
            .json(&json!({"ticker":"AAPL","strike_price":150.0,"expiry_date":"2025-12-19","open_date":"2025-01-10","premium_received":200.0,"fees_open":1.3})).await;
        let res = srv.get("/api/dashboard").await;
        res.assert_status_ok();
        let body = res.json::<serde_json::Value>();
        assert!(body["total_premium_collected"].as_f64().is_some());
        assert!(body["open_trades"].as_array().is_some());
    }
}
