use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::{LogEntry, OverviewStats};

#[derive(Debug, Clone)]
pub struct NewLog {
    pub user_id: i64,
    pub token_id: Option<i64>,
    pub channel_id: Option<i64>,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub quota: i64,
    pub duration_ms: i64,
    pub status: String,
    pub estimated: bool,
    pub error: Option<String>,
}

pub async fn insert_log(pool: &DbPool, entry: NewLog) -> AppResult<LogEntry> {
    let result = sqlx::query(
        r#"
        INSERT INTO logs
            (user_id, token_id, channel_id, model, prompt_tokens, completion_tokens,
             quota, duration_ms, status, estimated, error)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(entry.user_id)
    .bind(entry.token_id)
    .bind(entry.channel_id)
    .bind(&entry.model)
    .bind(entry.prompt_tokens)
    .bind(entry.completion_tokens)
    .bind(entry.quota)
    .bind(entry.duration_ms)
    .bind(&entry.status)
    .bind(if entry.estimated { 1 } else { 0 })
    .bind(&entry.error)
    .execute(pool)
    .await?;

    Ok(sqlx::query_as("SELECT * FROM logs WHERE id = ?")
        .bind(result.last_insert_rowid())
        .fetch_one(pool)
        .await?)
}

pub async fn list_logs(
    pool: &DbPool,
    user_id: Option<i64>,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<LogEntry>> {
    let limit = limit.clamp(1, 200);
    let offset = offset.max(0);
    if let Some(uid) = user_id {
        Ok(
            sqlx::query_as(
                "SELECT * FROM logs WHERE user_id = ? ORDER BY id DESC LIMIT ? OFFSET ?",
            )
            .bind(uid)
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await?,
        )
    } else {
        Ok(
            sqlx::query_as("SELECT * FROM logs ORDER BY id DESC LIMIT ? OFFSET ?")
                .bind(limit)
                .bind(offset)
                .fetch_all(pool)
                .await?,
        )
    }
}

pub async fn overview(pool: &DbPool) -> AppResult<OverviewStats> {
    let total_requests: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM logs")
        .fetch_one(pool)
        .await?;
    let success_requests: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM logs WHERE status = 'success'")
            .fetch_one(pool)
            .await?;
    let total_quota: (i64,) = sqlx::query_as("SELECT COALESCE(SUM(quota), 0) FROM logs")
        .fetch_one(pool)
        .await?;
    let total_users: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    let total_channels: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM channels")
        .fetch_one(pool)
        .await?;

    Ok(OverviewStats {
        total_requests: total_requests.0,
        success_requests: success_requests.0,
        total_quota: total_quota.0,
        total_users: total_users.0,
        total_channels: total_channels.0,
    })
}
