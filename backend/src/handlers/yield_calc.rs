use crate::models::share_lot::ShareLot;
use crate::models::trade::Trade;
use chrono::NaiveDate;
use sqlx::SqlitePool;

/// Result of yield calculations across a set of trades.
#[derive(Debug, Clone)]
pub struct YieldResult {
    pub realized_yield: f64,
    pub open_yield: f64,
}

/// Calculate capital-weighted annualized yields for a set of trades.
///
/// Returns realized (closed) and open annualized yields as percentages,
/// rounded to 2 decimal places. Used by both dashboard and statistics handlers.
pub async fn calculate_yields(pool: &SqlitePool, trades: &[Trade]) -> YieldResult {
    let mut realized_weighted = 0.0;
    let mut realized_capital = 0.0;
    let mut open_weighted = 0.0;
    let mut open_capital = 0.0;
    let today_str = chrono::Local::now().format("%Y-%m-%d").to_string();

    for trade in trades {
        let net = trade.net_premium().unwrap_or(0.0);
        let capital = get_capital_for_trade(pool, trade).await;

        if trade.status == "OPEN" {
            let days = days_between(&trade.open_date, &today_str);
            if capital > 0.0 {
                let ann = (net / capital) * (365.0 / days);
                open_weighted += ann * capital;
                open_capital += capital;
            }
        } else {
            let close_date = trade.close_date.as_deref().unwrap_or(&today_str);
            let days = days_between(&trade.open_date, close_date);
            if capital > 0.0 {
                let ann = (net / capital) * (365.0 / days);
                realized_weighted += ann * capital;
                realized_capital += capital;
            }
        }
    }

    let realized_yield = if realized_capital > 0.0 {
        round2((realized_weighted / realized_capital) * 100.0)
    } else {
        0.0
    };

    let open_yield = if open_capital > 0.0 {
        round2((open_weighted / open_capital) * 100.0)
    } else {
        0.0
    };

    YieldResult {
        realized_yield,
        open_yield,
    }
}

/// Calculate capital deployed for a trade.
/// For PUTs: strike_price * 100 * quantity
/// For CALLs: uses linked share lot's adjusted_cost_basis if available
pub async fn get_capital_for_trade(pool: &SqlitePool, trade: &Trade) -> f64 {
    let qty = trade.quantity as f64;
    if trade.trade_type == "CALL" {
        if let Some(lot_id) = trade.share_lot_id {
            if let Ok(lot) = ShareLot::get(pool, lot_id).await {
                return lot.adjusted_cost_basis * 100.0 * qty;
            }
        }
    }
    trade.strike_price * 100.0 * qty
}

pub fn days_between(from: &str, to: &str) -> f64 {
    let parse = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
    match (parse(from), parse(to)) {
        (Some(f), Some(t)) => (t - f).num_days().max(1) as f64,
        _ => 1.0,
    }
}

pub fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::account::Account;
    use crate::models::trade::CreateTrade;
    use crate::{db, models::trade::Trade as TradeModel};

    async fn setup() -> (SqlitePool, i64) {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let acct = Account::create(&pool, "Test").await.unwrap();
        (pool, acct.id)
    }

    #[test]
    fn test_days_between_basic() {
        assert!((days_between("2025-01-01", "2025-01-31") - 30.0).abs() < 0.001);
    }

    #[test]
    fn test_days_between_same_day_returns_one() {
        assert!((days_between("2025-01-01", "2025-01-01") - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_days_between_invalid_returns_one() {
        assert!((days_between("bad", "date") - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_round2() {
        assert!((round2(1.2345) - 1.23).abs() < 0.001);
        assert!((round2(1.235) - 1.24).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_yield_no_trades() {
        let (pool, _acct_id) = setup().await;
        let result = calculate_yields(&pool, &[]).await;
        assert!((result.realized_yield - 0.0).abs() < 0.001);
        assert!((result.open_yield - 0.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_yield_closed_trade() {
        let (pool, acct_id) = setup().await;

        let input = CreateTrade {
            account_id: acct_id,
            trade_type: "PUT".to_string(),
            ticker: "AAPL".to_string(),
            strike_price: 150.0,
            expiry_date: "2025-02-21".to_string(),
            open_date: "2025-01-10".to_string(),
            premium_received: 200.0,
            fees_open: 1.3,
            share_lot_id: None,
            quantity: Some(1),
        };
        let trade = TradeModel::create(&pool, &input).await.unwrap();
        let closed = TradeModel::close(
            &pool,
            trade.id,
            "EXPIRED",
            None,
            None,
            Some("2025-02-21".to_string()),
        )
        .await
        .unwrap();

        let result = calculate_yields(&pool, &[closed]).await;

        // net_premium = 200 - 1.3 = 198.70
        // capital = 150 * 100 = 15000
        // days = 42
        // annualized = (198.70 / 15000) * (365 / 42) = 0.11511...
        // yield = 11.51%
        assert!(result.realized_yield > 11.0 && result.realized_yield < 12.0);
        assert!((result.open_yield - 0.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_yield_open_trade() {
        let (pool, acct_id) = setup().await;

        let input = CreateTrade {
            account_id: acct_id,
            trade_type: "PUT".to_string(),
            ticker: "AAPL".to_string(),
            strike_price: 150.0,
            expiry_date: "2025-12-19".to_string(),
            open_date: "2025-01-10".to_string(),
            premium_received: 200.0,
            fees_open: 1.3,
            share_lot_id: None,
            quantity: Some(1),
        };
        let trade = TradeModel::create(&pool, &input).await.unwrap();

        let result = calculate_yields(&pool, &[trade]).await;

        assert!((result.realized_yield - 0.0).abs() < 0.001);
        assert!(result.open_yield > 0.0);
    }

    #[tokio::test]
    async fn test_yield_matches_dashboard_and_statistics() {
        // Verify that the shared function produces consistent results
        // for a mix of open and closed trades
        let (pool, acct_id) = setup().await;

        let put1 = TradeModel::create(
            &pool,
            &CreateTrade {
                account_id: acct_id,
                trade_type: "PUT".to_string(),
                ticker: "AAPL".to_string(),
                strike_price: 150.0,
                expiry_date: "2025-02-21".to_string(),
                open_date: "2025-01-10".to_string(),
                premium_received: 200.0,
                fees_open: 1.3,
                share_lot_id: None,
                quantity: Some(1),
            },
        )
        .await
        .unwrap();
        let closed1 = TradeModel::close(
            &pool,
            put1.id,
            "EXPIRED",
            None,
            None,
            Some("2025-02-21".to_string()),
        )
        .await
        .unwrap();

        let open1 = TradeModel::create(
            &pool,
            &CreateTrade {
                account_id: acct_id,
                trade_type: "PUT".to_string(),
                ticker: "MSFT".to_string(),
                strike_price: 300.0,
                expiry_date: "2025-12-19".to_string(),
                open_date: "2025-06-01".to_string(),
                premium_received: 500.0,
                fees_open: 2.0,
                share_lot_id: None,
                quantity: Some(1),
            },
        )
        .await
        .unwrap();

        let trades = vec![closed1, open1];
        let result = calculate_yields(&pool, &trades).await;

        // Both yields should be non-zero
        assert!(result.realized_yield > 0.0);
        assert!(result.open_yield > 0.0);

        // Running again with same trades should produce identical results
        let result2 = calculate_yields(&pool, &trades).await;
        assert!((result.realized_yield - result2.realized_yield).abs() < 0.001);
        assert!((result.open_yield - result2.open_yield).abs() < 0.001);
    }
}
