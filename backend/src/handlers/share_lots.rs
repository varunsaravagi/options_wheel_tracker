use axum::{extract::{Path, State}, http::StatusCode, Json};
use serde::Deserialize;
use sqlx::SqlitePool;
use crate::errors::AppError;
use crate::models::share_lot::{CreateShareLot, ShareLot};

#[derive(Deserialize)]
pub struct CreateManualLot {
    pub ticker: String,
    pub cost_basis: f64,
    pub acquisition_date: String,
}

pub async fn create_manual_lot(
    State(pool): State<SqlitePool>,
    Path(account_id): Path<i64>,
    Json(payload): Json<CreateManualLot>,
) -> Result<(StatusCode, Json<ShareLot>), AppError> {
    if payload.cost_basis <= 0.0 {
        return Err(AppError::BadRequest("cost_basis must be positive".to_string()));
    }
    let lot = ShareLot::create(&pool, &CreateShareLot {
        account_id,
        ticker: payload.ticker.to_uppercase(),
        original_cost_basis: payload.cost_basis,
        adjusted_cost_basis: None,
        acquisition_date: payload.acquisition_date,
        acquisition_type: "MANUAL".to_string(),
        source_trade_id: None,
    }).await?;
    Ok((StatusCode::CREATED, Json(lot)))
}

#[cfg(test)]
mod tests {
    use axum_test::TestServer;
    use axum::http::StatusCode;
    use serde_json::json;
    use crate::{db, routes::create_router};

    #[tokio::test]
    async fn test_create_manual_lot() {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let srv = TestServer::new(create_router(pool)).unwrap();
        let acct_id = srv.post("/api/accounts").json(&json!({"name":"T"})).await
            .json::<serde_json::Value>()["id"].as_i64().unwrap();
        let res = srv.post(&format!("/api/accounts/{}/share-lots", acct_id))
            .json(&json!({
                "ticker": "MSFT",
                "cost_basis": 300.00,
                "acquisition_date": "2024-06-01"
            })).await;
        res.assert_status(StatusCode::CREATED);
        let body = res.json::<serde_json::Value>();
        assert_eq!(body["ticker"], "MSFT");
        assert_eq!(body["acquisition_type"], "MANUAL");
        assert_eq!(body["adjusted_cost_basis"].as_f64().unwrap(), 300.00);
    }
}
