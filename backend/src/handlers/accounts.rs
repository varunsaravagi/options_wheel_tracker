use axum::{extract::{Path, State}, http::StatusCode, Json};
use sqlx::SqlitePool;
use crate::{errors::AppError, models::account::{Account, CreateAccount}};

pub async fn list_accounts(
    State(pool): State<SqlitePool>,
) -> Result<Json<Vec<Account>>, AppError> {
    Account::list(&pool).await.map(Json)
}

pub async fn create_account(
    State(pool): State<SqlitePool>,
    Json(payload): Json<CreateAccount>,
) -> Result<(StatusCode, Json<Account>), AppError> {
    if payload.name.trim().is_empty() {
        return Err(AppError::BadRequest("name cannot be empty".to_string()));
    }
    Account::create(&pool, &payload.name)
        .await
        .map(|a| (StatusCode::CREATED, Json(a)))
}

pub async fn delete_account(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    Account::delete(&pool, id).await?;
    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use axum_test::TestServer;
    use serde_json::json;
    use crate::routes::create_router;
    use crate::db;

    async fn test_server() -> TestServer {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        TestServer::new(create_router(pool)).unwrap()
    }

    #[tokio::test]
    async fn test_create_account() {
        let server = test_server().await;
        let res = server.post("/api/accounts")
            .json(&json!({ "name": "Fidelity" }))
            .await;
        res.assert_status(axum::http::StatusCode::CREATED);
        let body = res.json::<serde_json::Value>();
        assert_eq!(body["name"], "Fidelity");
        assert!(body["id"].is_number());
    }

    #[tokio::test]
    async fn test_list_accounts() {
        let server = test_server().await;
        server.post("/api/accounts").json(&json!({ "name": "Fidelity" })).await;
        let res = server.get("/api/accounts").await;
        res.assert_status_ok();
        let body = res.json::<serde_json::Value>();
        assert_eq!(body.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_delete_account() {
        let server = test_server().await;
        let create = server.post("/api/accounts")
            .json(&json!({ "name": "TDA" })).await;
        let id = create.json::<serde_json::Value>()["id"].as_i64().unwrap();
        let res = server.delete(&format!("/api/accounts/{}", id)).await;
        res.assert_status_ok();
    }
}
