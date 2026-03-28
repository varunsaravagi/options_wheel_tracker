mod config;
mod db;
mod errors;
mod handlers;
mod models;
mod routes;

use dotenvy::dotenv;
use std::env;

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::fmt::init();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let port = env::var("BACKEND_PORT").unwrap_or_else(|_| "3003".to_string());

    let pool = db::init_pool(&database_url).await;
    db::run_migrations(&pool).await;

    let app = routes::create_router(pool);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .unwrap();

    tracing::info!("Listening on port {}", port);
    axum::serve(listener, app).await.unwrap();
}
