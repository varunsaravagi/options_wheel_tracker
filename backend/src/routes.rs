use crate::handlers::{accounts, calls, dashboard, history, puts, share_lots, statistics};
use axum::{
    routing::{delete, get, post, put},
    Router,
};
use sqlx::SqlitePool;
use tower_http::cors::CorsLayer;

pub fn create_router(pool: SqlitePool) -> Router {
    Router::new()
        .route(
            "/api/accounts",
            get(accounts::list_accounts).post(accounts::create_account),
        )
        .route("/api/accounts/:id", delete(accounts::delete_account))
        .route(
            "/api/accounts/:id/purge",
            delete(accounts::purge_account_data),
        )
        .route("/api/accounts/:id/puts", post(puts::open_put))
        .route("/api/trades/puts/:id/close", post(puts::close_put))
        .route("/api/trades/:id", put(puts::edit_trade))
        .route("/api/accounts/:id/calls", post(calls::open_call))
        .route(
            "/api/accounts/:id/share-lots",
            get(calls::list_share_lots).post(share_lots::create_manual_lot),
        )
        .route("/api/share-lots/:id/sell", put(share_lots::sell_share_lot))
        .route("/api/trades/calls/:id/close", post(calls::close_call))
        .route("/api/dashboard", get(dashboard::get_dashboard))
        .route("/api/history", get(history::get_history))
        .route("/api/statistics", get(statistics::get_statistics))
        .layer(CorsLayer::permissive())
        .with_state(pool)
}
