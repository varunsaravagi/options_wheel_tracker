use crate::{errors::AppError, models::trade::Trade};
use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use sqlx::SqlitePool;

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub account_id: Option<i64>,
    pub ticker: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
}

pub async fn get_history(
    State(pool): State<SqlitePool>,
    Query(params): Query<HistoryQuery>,
) -> Result<Json<Vec<Trade>>, AppError> {
    Trade::list_with_filters(
        &pool,
        params.account_id,
        params.ticker.as_deref(),
        params.date_from.as_deref(),
        params.date_to.as_deref(),
    )
    .await
    .map(Json)
}
