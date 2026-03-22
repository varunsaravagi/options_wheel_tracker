use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

pub type Pool = SqlitePool;

pub async fn init_pool(database_url: &str) -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .expect("Failed to connect to database")
}

pub async fn run_migrations(pool: &SqlitePool) {
    sqlx::migrate!("src/db/migrations")
        .run(pool)
        .await
        .expect("Failed to run migrations");
}
