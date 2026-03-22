use axum::{routing::{delete, get}, Router};
use sqlx::SqlitePool;
use tower_http::cors::CorsLayer;
use crate::handlers::accounts;

pub fn create_router(pool: SqlitePool) -> Router {
    Router::new()
        .route("/api/accounts", get(accounts::list_accounts).post(accounts::create_account))
        .route("/api/accounts/:id", delete(accounts::delete_account))
        .layer(CorsLayer::permissive())
        .with_state(pool)
}
