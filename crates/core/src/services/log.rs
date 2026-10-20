use crate::db::DbPool;
use crate::error::AppResult;
use crate::models::{LogEntry, OverviewStats};
use crate::usage::Usage;
use sqlx::QueryBuilder;

#[derive(Debug, Clone, Default)]
pub struct LogQuery {
    pub user_id: Option<i64>,
    pub model: Option<String>,
    pub status: Option<String>,
    pub channel_id: Option<i64>,
    pub limit: i64,
    pub offset: i64,
}

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
    pub error: Option<String>,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub reasoning_tokens: i64,
    pub usage_adapter: String,
    pub raw_usage: Option<String>,
}

impl NewLog {
    pub fn new(user_id: i64, token_id: Option<i64>, model: &str) -> Self {
        Self {
            user_id,
            token_id,
            channel_id: None,
            model: model.to_string(),
            prompt_tokens: 0,
            completion_tokens: 0,
            quota: 0,
            duration_ms: 0,
            status: String::new(),
            error: None,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            usage_adapter: String::new(),
            raw_usage: None,
        }
    }

    pub fn with_usage(mut self, usage: &Usage) -> Self {
        self.prompt_tokens = usage.input_tokens;
        self.completion_tokens = usage.output_tokens;
        self.cache_read_tokens = usage.cache_read_tokens;
        self.cache_write_tokens = usage.cache_write_tokens;
        self.reasoning_tokens = usage.reasoning_tokens;
        self.usage_adapter = usage.adapter.clone();
        self
    }
}

pub async fn insert_log(pool: &DbPool, entry: NewLog) -> AppResult<LogEntry> {
    let result = sqlx::query(
        r#"
        INSERT INTO logs
            (user_id, token_id, channel_id, model, prompt_tokens, completion_tokens,
             quota, duration_ms, status, estimated, error,
             cache_read_tokens, cache_write_tokens, reasoning_tokens, usage_adapter, raw_usage)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?)
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
    .bind(&entry.error)
    .bind(entry.cache_read_tokens)
    .bind(entry.cache_write_tokens)
    .bind(entry.reasoning_tokens)
    .bind(&entry.usage_adapter)
    .bind(&entry.raw_usage)
    .execute(pool)
    .await?;

    Ok(sqlx::query_as("SELECT * FROM logs WHERE id = ?")
        .bind(result.last_insert_rowid())
        .fetch_one(pool)
        .await?)
}

pub async fn list_logs(pool: &DbPool, query: &LogQuery) -> AppResult<Vec<LogEntry>> {
    let limit = if query.limit <= 0 {
        50
    } else {
        query.limit.min(200)
    };
    let offset = query.offset.max(0);
    let mut qb: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new("SELECT * FROM logs WHERE 1 = 1");
    if let Some(uid) = query.user_id {
        qb.push(" AND user_id = ").push_bind(uid);
    }
    if let Some(model) = nonempty(&query.model) {
        qb.push(" AND model = ").push_bind(model);
    }
    if let Some(status) = nonempty(&query.status) {
        qb.push(" AND status = ").push_bind(status);
    }
    if let Some(channel_id) = query.channel_id {
        qb.push(" AND channel_id = ").push_bind(channel_id);
    }
    qb.push(" ORDER BY id DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);
    Ok(qb.build_query_as().fetch_all(pool).await?)
}

fn nonempty(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
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
    let today_requests: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM logs WHERE created_at >= date('now')")
            .fetch_one(pool)
            .await?;
    let failed_requests: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM logs WHERE status != 'success'")
            .fetch_one(pool)
            .await?;
    let today_quota: (i64,) =
        sqlx::query_as("SELECT COALESCE(SUM(quota), 0) FROM logs WHERE created_at >= date('now')")
            .fetch_one(pool)
            .await?;

    Ok(OverviewStats {
        total_requests: total_requests.0,
        success_requests: success_requests.0,
        total_quota: total_quota.0,
        total_users: total_users.0,
        total_channels: total_channels.0,
        today_requests: today_requests.0,
        failed_requests: failed_requests.0,
        today_quota: today_quota.0,
    })
}
