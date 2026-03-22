use axum::Router;
use sqlx::SqlitePool;
use tower_http::cors::CorsLayer;

pub fn create_router(pool: SqlitePool) -> Router {
    Router::new()
        .layer(CorsLayer::permissive())
        .with_state(pool)
}
