use axum::{routing::{delete, get, post}, Router};
use sqlx::SqlitePool;
use tower_http::cors::CorsLayer;
use crate::handlers::{accounts, calls, puts, share_lots};

pub fn create_router(pool: SqlitePool) -> Router {
    Router::new()
        .route("/api/accounts", get(accounts::list_accounts).post(accounts::create_account))
        .route("/api/accounts/:id", delete(accounts::delete_account))
        .route("/api/accounts/:id/puts", post(puts::open_put))
        .route("/api/trades/puts/:id/close", post(puts::close_put))
        .route("/api/accounts/:id/calls", post(calls::open_call))
        .route("/api/accounts/:id/share-lots", get(calls::list_share_lots).post(share_lots::create_manual_lot))
        .route("/api/trades/calls/:id/close", post(calls::close_call))
        .layer(CorsLayer::permissive())
        .with_state(pool)
}
