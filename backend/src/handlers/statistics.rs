use crate::errors::AppError;
use crate::handlers::yield_calc::{calculate_yields, round2};
use crate::models::trade::Trade;
use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::BTreeMap;

#[derive(Deserialize)]
pub struct StatisticsQuery {
    pub account_id: Option<i64>,
}

#[derive(Serialize)]
pub struct MonthlyIncome {
    pub month: String,
    pub sto_income: f64,
    pub btc_cost: f64,
    pub net_income: f64,
}

#[derive(Serialize)]
pub struct CumulativePnl {
    pub month: String,
    pub cumulative: f64,
}

#[derive(Serialize)]
pub struct TickerPremium {
    pub ticker: String,
    pub net_premium: f64,
}

#[derive(Serialize)]
pub struct StatisticsResponse {
    pub total_premium: f64,
    pub total_premium_open: f64,
    pub yield_closed: f64,
    pub yield_open: f64,
    pub monthly_income: Vec<MonthlyIncome>,
    pub cumulative_pnl: Vec<CumulativePnl>,
    pub premium_by_ticker: Vec<TickerPremium>,
}

fn month_key(date: &str) -> String {
    // "2025-01-15" -> "2025-01"
    date.get(..7).unwrap_or(date).to_string()
}

pub async fn get_statistics(
    State(pool): State<SqlitePool>,
    Query(params): Query<StatisticsQuery>,
) -> Result<Json<StatisticsResponse>, AppError> {
    let trades = Trade::list_with_filters(&pool, params.account_id, None, None, None).await?;

    let mut total_premium = 0.0;
    let mut total_premium_open = 0.0;

    // Monthly income: keyed by month string
    let mut monthly_sto: BTreeMap<String, f64> = BTreeMap::new();
    let mut monthly_btc: BTreeMap<String, f64> = BTreeMap::new();

    // Premium by ticker (closed trades only)
    let mut ticker_premium: BTreeMap<String, f64> = BTreeMap::new();

    for trade in &trades {
        let net = trade.net_premium().unwrap_or(0.0);

        // STO income: premium received when opening the trade
        let open_month = month_key(&trade.open_date);
        *monthly_sto.entry(open_month).or_insert(0.0) += trade.premium_received - trade.fees_open;

        if trade.status == "OPEN" {
            total_premium_open += net;
        } else {
            total_premium += net;

            // BTC cost: close_premium when buying back
            if let (Some(cp), Some(ref cd)) = (trade.close_premium, &trade.close_date) {
                if cp > 0.0 {
                    let close_month = month_key(cd);
                    *monthly_btc.entry(close_month).or_insert(0.0) +=
                        cp + trade.fees_close.unwrap_or(0.0);
                }
            }

            // Ticker premium (closed trades)
            *ticker_premium.entry(trade.ticker.clone()).or_insert(0.0) += net;
        }
    }

    // Merge all months
    let mut all_months: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for (m, v) in &monthly_sto {
        all_months.entry(m.clone()).or_insert((0.0, 0.0)).0 += v;
    }
    for (m, v) in &monthly_btc {
        all_months.entry(m.clone()).or_insert((0.0, 0.0)).1 += v;
    }

    let monthly_income: Vec<MonthlyIncome> = all_months
        .iter()
        .map(|(month, (sto, btc))| MonthlyIncome {
            month: month.clone(),
            sto_income: round2(*sto),
            btc_cost: round2(*btc),
            net_income: round2(sto - btc),
        })
        .collect();

    // Cumulative P&L
    let mut cumulative = 0.0;
    let cumulative_pnl: Vec<CumulativePnl> = monthly_income
        .iter()
        .map(|mi| {
            cumulative += mi.net_income;
            CumulativePnl {
                month: mi.month.clone(),
                cumulative: round2(cumulative),
            }
        })
        .collect();

    // Premium by ticker sorted by premium descending
    let mut premium_by_ticker: Vec<TickerPremium> = ticker_premium
        .into_iter()
        .map(|(ticker, net_premium)| TickerPremium {
            ticker,
            net_premium: round2(net_premium),
        })
        .collect();
    premium_by_ticker.sort_by(|a, b| b.net_premium.partial_cmp(&a.net_premium).unwrap());

    let yields = calculate_yields(&pool, &trades).await;
    let yield_closed = yields.realized_yield;
    let yield_open = yields.open_yield;

    Ok(Json(StatisticsResponse {
        total_premium: round2(total_premium),
        total_premium_open: round2(total_premium_open),
        yield_closed,
        yield_open,
        monthly_income,
        cumulative_pnl,
        premium_by_ticker,
    }))
}

#[cfg(test)]
mod tests {
    use crate::{db, routes::create_router};
    use axum_test::TestServer;
    use serde_json::json;

    #[tokio::test]
    async fn test_statistics_empty() {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let srv = TestServer::new(create_router(pool)).unwrap();

        let res = srv.get("/api/statistics").await;
        res.assert_status_ok();
        let body = res.json::<serde_json::Value>();
        assert_eq!(body["total_premium"], 0.0);
        assert_eq!(body["monthly_income"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_statistics_with_trades() {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let srv = TestServer::new(create_router(pool)).unwrap();

        let acct_id = srv
            .post("/api/accounts")
            .json(&json!({"name":"Test"}))
            .await
            .json::<serde_json::Value>()["id"]
            .as_i64()
            .unwrap();

        // Open a PUT
        let put = srv
            .post(&format!("/api/accounts/{}/puts", acct_id))
            .json(&json!({
                "ticker": "AAPL",
                "strike_price": 150.0,
                "expiry_date": "2025-02-21",
                "open_date": "2025-01-10",
                "premium_received": 200.0,
                "fees_open": 1.3
            }))
            .await
            .json::<serde_json::Value>();
        let put_id = put["id"].as_i64().unwrap();

        // Close it as expired
        srv.post(&format!("/api/trades/puts/{}/close", put_id))
            .json(&json!({
                "action": "EXPIRED",
                "close_date": "2025-02-21"
            }))
            .await;

        let res = srv.get("/api/statistics").await;
        res.assert_status_ok();
        let body = res.json::<serde_json::Value>();

        // Net premium: 200 - 1.3 = 198.70
        assert!((body["total_premium"].as_f64().unwrap() - 198.70).abs() < 0.01);
        assert!(body["monthly_income"].as_array().unwrap().len() > 0);
        assert!(body["premium_by_ticker"].as_array().unwrap().len() > 0);

        let ticker = &body["premium_by_ticker"][0];
        assert_eq!(ticker["ticker"], "AAPL");
    }

    #[tokio::test]
    async fn test_statistics_account_filter() {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let srv = TestServer::new(create_router(pool)).unwrap();

        let acct1 = srv
            .post("/api/accounts")
            .json(&json!({"name":"A1"}))
            .await
            .json::<serde_json::Value>()["id"]
            .as_i64()
            .unwrap();
        let acct2 = srv
            .post("/api/accounts")
            .json(&json!({"name":"A2"}))
            .await
            .json::<serde_json::Value>()["id"]
            .as_i64()
            .unwrap();

        // Trade on account 1
        let p1 = srv
            .post(&format!("/api/accounts/{}/puts", acct1))
            .json(&json!({
                "ticker": "AAPL", "strike_price": 150.0,
                "expiry_date": "2025-02-21", "open_date": "2025-01-10",
                "premium_received": 200.0, "fees_open": 1.0
            }))
            .await
            .json::<serde_json::Value>()["id"]
            .as_i64()
            .unwrap();
        srv.post(&format!("/api/trades/puts/{}/close", p1))
            .json(&json!({"action": "EXPIRED", "close_date": "2025-02-21"}))
            .await;

        // Trade on account 2
        let p2 = srv
            .post(&format!("/api/accounts/{}/puts", acct2))
            .json(&json!({
                "ticker": "MSFT", "strike_price": 300.0,
                "expiry_date": "2025-03-21", "open_date": "2025-02-10",
                "premium_received": 500.0, "fees_open": 2.0
            }))
            .await
            .json::<serde_json::Value>()["id"]
            .as_i64()
            .unwrap();
        srv.post(&format!("/api/trades/puts/{}/close", p2))
            .json(&json!({"action": "EXPIRED", "close_date": "2025-03-21"}))
            .await;

        // Filter by account 1 only
        let res = srv
            .get(&format!("/api/statistics?account_id={}", acct1))
            .await;
        res.assert_status_ok();
        let body = res.json::<serde_json::Value>();
        assert!((body["total_premium"].as_f64().unwrap() - 199.0).abs() < 0.01);
        assert_eq!(body["premium_by_ticker"].as_array().unwrap().len(), 1);
        assert_eq!(body["premium_by_ticker"][0]["ticker"], "AAPL");
    }
}
