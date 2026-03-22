use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use crate::errors::AppError;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateAccount {
    pub name: String,
}

impl Account {
    pub async fn create(pool: &SqlitePool, name: &str) -> Result<Account, AppError> {
        let account = sqlx::query_as::<_, Account>(
            "INSERT INTO accounts (name) VALUES (?) RETURNING id, name, created_at"
        )
        .bind(name)
        .fetch_one(pool)
        .await?;
        Ok(account)
    }

    pub async fn list(pool: &SqlitePool) -> Result<Vec<Account>, AppError> {
        let accounts = sqlx::query_as::<_, Account>(
            "SELECT id, name, created_at FROM accounts ORDER BY created_at"
        )
        .fetch_all(pool)
        .await?;
        Ok(accounts)
    }

    pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), AppError> {
        let result = sqlx::query("DELETE FROM accounts WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }
}
