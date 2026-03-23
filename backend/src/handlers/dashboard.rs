use axum::{extract::{Query, State}, Json};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use crate::errors::AppError;
use crate::models::share_lot::ShareLot;
use crate::models::trade::Trade;

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

fn days_between(from: &str, to: &str) -> f64 {
    let parse = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
    match (parse(from), parse(to)) {
        (Some(f), Some(t)) => (t - f).num_days().max(1) as f64,
        _ => 1.0,
    }
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

async fn get_capital_for_trade(pool: &SqlitePool, trade: &Trade) -> f64 {
    let qty = trade.quantity as f64;
    if trade.trade_type == "PUT" {
        trade.strike_price * 100.0 * qty
    } else {
        // CALL: look up linked lot's adjusted_cost_basis
        if let Some(lot_id) = trade.share_lot_id {
            if let Ok(lot) = ShareLot::get(pool, lot_id).await {
                return lot.adjusted_cost_basis * 100.0 * qty;
            }
        }
        trade.strike_price * 100.0 * qty
    }
}

pub async fn get_dashboard(
    State(pool): State<SqlitePool>,
    Query(params): Query<DashboardQuery>,
) -> Result<Json<DashboardResponse>, AppError> {
    let trades = Trade::list_with_filters(&pool, params.account_id, None, None, None).await?;

    let mut total_premium = 0.0;
    let mut open_trades = Vec::new();

    // For weighted average: sum of (yield * capital) and sum of capital
    let mut realized_weighted_sum = 0.0;
    let mut realized_capital_sum = 0.0;
    let mut open_weighted_sum = 0.0;
    let mut open_capital_sum = 0.0;
    let mut total_capital_deployed = 0.0;

    let today_str = today();

    for trade in &trades {
        let net = trade.net_premium().unwrap_or(0.0);
        let capital = get_capital_for_trade(&pool, trade).await;

        if trade.status == "OPEN" {
            let days = days_between(&trade.open_date, &today_str);
            let annualized = if capital > 0.0 { (net / capital) * (365.0 / days) } else { 0.0 };
            open_weighted_sum += annualized * capital;
            open_capital_sum += capital;
            open_trades.push(trade.clone());
            total_capital_deployed += capital;
        } else {
            // Closed trade
            total_premium += net;
            let close_date = trade.close_date.as_deref().unwrap_or(&today_str);
            let days = days_between(&trade.open_date, close_date);
            let annualized = if capital > 0.0 { (net / capital) * (365.0 / days) } else { 0.0 };
            realized_weighted_sum += annualized * capital;
            realized_capital_sum += capital;
        }
    }

    let realized_annualized_yield = if realized_capital_sum > 0.0 {
        (realized_weighted_sum / realized_capital_sum) * 100.0
    } else {
        0.0
    };

    let open_annualized_yield = if open_capital_sum > 0.0 {
        (open_weighted_sum / open_capital_sum) * 100.0
    } else {
        0.0
    };

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

    // Round yields to 2 decimal places
    let realized_annualized_yield = (realized_annualized_yield * 100.0).round() / 100.0;
    let open_annualized_yield = (open_annualized_yield * 100.0).round() / 100.0;
    let total_premium = (total_premium * 100.0).round() / 100.0;
    let total_capital_deployed = (total_capital_deployed * 100.0).round() / 100.0;

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
    use axum_test::TestServer;
    use serde_json::json;
    use crate::{db, routes::create_router};

    #[tokio::test]
    async fn test_dashboard_totals() {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let srv = TestServer::new(create_router(pool)).unwrap();
        let acct_id = srv.post("/api/accounts").json(&json!({"name":"T"})).await
            .json::<serde_json::Value>()["id"].as_i64().unwrap();
        srv.post(&format!("/api/accounts/{}/puts", acct_id))
            .json(&json!({"ticker":"AAPL","strike_price":150.0,"expiry_date":"2025-12-19","open_date":"2025-01-10","premium_received":200.0,"fees_open":1.3})).await;
        let res = srv.get("/api/dashboard").await;
        res.assert_status_ok();
        let body = res.json::<serde_json::Value>();
        assert!(body["total_premium_collected"].as_f64().is_some());
        assert!(body["open_trades"].as_array().is_some());
    }
}
