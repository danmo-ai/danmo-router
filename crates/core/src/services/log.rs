use crate::db::{self, DbPool};
use crate::error::{AppError, AppResult};
use crate::models::{
    LogEntry, OverviewStats, UsageBucket, UsageByModel, UsageDay, UsageReport,
};
use crate::usage::Usage;
use chrono::{Duration, NaiveDate, Utc};
use sqlx::QueryBuilder;
use tracing::{info, warn};

const MAX_USAGE_SPAN_DAYS: i64 = 365;
const ARCHIVE_BATCH_SIZE: i64 = 2000;
const DAY_EXPR: &str = "SUBSTR(created_at, 1, 10)";

/// Column widths of `logs.error` / `logs.raw_usage` in the MySQL schema.
const MAX_ERROR_CHARS: usize = 2000;
const MAX_RAW_USAGE_CHARS: usize = 8000;

#[derive(Debug, Clone, Default)]
pub struct LogQuery {
    pub user_id: Option<i64>,
    pub token_id: Option<i64>,
    pub model: Option<String>,
    pub status: Option<String>,
    pub channel_id: Option<i64>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Default)]
pub struct LogFilter {
    pub user_id: Option<i64>,
    pub token_id: Option<i64>,
    pub channel_id: Option<i64>,
    pub model: Option<String>,
    pub status: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
}

impl From<&LogQuery> for LogFilter {
    fn from(q: &LogQuery) -> Self {
        Self {
            user_id: q.user_id,
            token_id: q.token_id,
            channel_id: q.channel_id,
            model: q.model.clone(),
            status: q.status.clone(),
            start: q.start.clone(),
            end: q.end.clone(),
        }
    }
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

impl LogFilter {
    fn push_predicates<'a>(&'a self, qb: &mut QueryBuilder<'a, sqlx::Any>, table: &str) {
        if let Some(uid) = self.user_id {
            qb.push(" AND ").push(table).push(".user_id = ").push_bind(uid);
        }
        if let Some(tid) = self.token_id {
            qb.push(" AND ").push(table).push(".token_id = ").push_bind(tid);
        }
        if let Some(cid) = self.channel_id {
            qb.push(" AND ").push(table).push(".channel_id = ").push_bind(cid);
        }
        if let Some(model) = nonempty(&self.model) {
            qb.push(" AND ").push(table).push(".model = ").push_bind(model);
        }
        if let Some(status) = nonempty(&self.status) {
            qb.push(" AND ").push(table).push(".status = ").push_bind(status);
        }
        if let Some(ref start) = self.start {
            if table == "logs" {
                qb.push(" AND ")
                    .push(table)
                    .push(".created_at >= ")
                    .push_bind(format!("{start} 00:00:00"));
            } else {
                qb.push(" AND ").push(table).push(".day >= ").push_bind(start.clone());
            }
        }
        if let Some(ref end) = self.end {
            if table == "logs" {
                qb.push(" AND ")
                    .push(table)
                    .push(".created_at <= ")
                    .push_bind(format!("{end} 23:59:59"));
            } else {
                qb.push(" AND ").push(table).push(".day <= ").push_bind(end.clone());
            }
        }
    }
}

fn nonempty(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub async fn insert_log(pool: &DbPool, mut entry: NewLog) -> AppResult<LogEntry> {
    if let Some(error) = &mut entry.error {
        if let Some((cut, _)) = error.char_indices().nth(MAX_ERROR_CHARS) {
            error.truncate(cut);
        }
    }
    if entry
        .raw_usage
        .as_ref()
        .is_some_and(|raw| raw.chars().count() > MAX_RAW_USAGE_CHARS)
    {
        entry.raw_usage = None;
    }
    let engine = db::engine(pool);
    let sql = format!(
        r#"
        INSERT INTO logs
            (user_id, token_id, channel_id, model, prompt_tokens, completion_tokens,
             quota, duration_ms, status, estimated, error,
             cache_read_tokens, cache_write_tokens, reasoning_tokens, usage_adapter, raw_usage,
             created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?){}
        "#,
        engine.returning_id()
    );
    let query = sqlx::query(&sql)
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
        .bind(db::now());
    let id = db::insert_id(engine, query, pool).await?;

    Ok(sqlx::query_as("SELECT * FROM logs WHERE id = ?")
        .bind(id)
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
    let filter = LogFilter::from(query);
    let mut qb: QueryBuilder<sqlx::Any> = QueryBuilder::new("SELECT * FROM logs WHERE 1 = 1");
    filter.push_predicates(&mut qb, "logs");
    qb.push(" ORDER BY id DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);
    Ok(qb.build_query_as().fetch_all(pool).await?)
}

pub async fn overview(pool: &DbPool) -> AppResult<OverviewStats> {
    let daily: (i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            CAST(COALESCE(SUM(requests), 0) AS SIGNED),
            CAST(COALESCE(SUM(CASE WHEN status = 'success' THEN requests ELSE 0 END), 0) AS SIGNED),
            CAST(COALESCE(SUM(quota), 0) AS SIGNED)
        FROM log_daily
        "#,
    )
    .fetch_one(pool)
    .await?;

    let hot: (i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            COUNT(*),
            CAST(COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) AS SIGNED),
            CAST(COALESCE(SUM(quota), 0) AS SIGNED)
        FROM logs
        "#,
    )
    .fetch_one(pool)
    .await?;

    let total_users: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    let total_channels: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM channels")
        .fetch_one(pool)
        .await?;

    let today = db::today();
    let today_requests: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM logs WHERE created_at >= ?")
        .bind(&today)
        .fetch_one(pool)
        .await?;
    let today_quota: (i64,) = sqlx::query_as(
        "SELECT CAST(COALESCE(SUM(quota), 0) AS SIGNED) FROM logs WHERE created_at >= ?",
    )
    .bind(&today)
    .fetch_one(pool)
    .await?;

    let total_requests = daily.0 + hot.0;
    let success_requests = daily.1 + hot.1;

    Ok(OverviewStats {
        total_requests,
        success_requests,
        total_quota: daily.2 + hot.2,
        total_users: total_users.0,
        total_channels: total_channels.0,
        today_requests: today_requests.0,
        failed_requests: total_requests - success_requests,
        today_quota: today_quota.0,
    })
}

pub fn normalize_range(
    start: Option<&str>,
    end: Option<&str>,
) -> AppResult<(NaiveDate, NaiveDate)> {
    let today = Utc::now().date_naive();
    let end = match end {
        Some(s) => parse_day(s)?,
        None => today,
    };
    let start = match start {
        Some(s) => parse_day(s)?,
        None => end - Duration::days(6),
    };
    if start > end {
        return Err(AppError::BadRequest("start must be on or before end".into()));
    }
    let mut start = start;
    if (end - start).num_days() > MAX_USAGE_SPAN_DAYS - 1 {
        start = end - Duration::days(MAX_USAGE_SPAN_DAYS - 1);
    }
    Ok((start, end))
}

fn parse_day(s: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest(format!("invalid date '{s}', expected YYYY-MM-DD")))
}

pub async fn usage_report(
    pool: &DbPool,
    filter: &LogFilter,
    retention_days: i64,
) -> AppResult<UsageReport> {
    let (start, end) = normalize_range(filter.start.as_deref(), filter.end.as_deref())?;
    let mut filter = filter.clone();
    filter.start = Some(start.format("%Y-%m-%d").to_string());
    filter.end = Some(end.format("%Y-%m-%d").to_string());

    let daily_rows = query_daily_by_day(pool, &filter).await?;
    let hot_rows = query_hot_by_day(pool, &filter).await?;
    let by_model = query_by_model(pool, &filter).await?;

    let mut by_day: std::collections::BTreeMap<String, UsageBucket> =
        std::collections::BTreeMap::new();
    for (day, bucket) in daily_rows.into_iter().chain(hot_rows) {
        let entry = by_day.entry(day).or_default();
        add_bucket(entry, &bucket);
    }

    let mut series = Vec::new();
    let mut totals = UsageBucket::default();
    let mut day = start;
    while day <= end {
        let key = day.format("%Y-%m-%d").to_string();
        let stats = by_day.remove(&key).unwrap_or_default();
        add_bucket(&mut totals, &stats);
        series.push(UsageDay { day: key, stats });
        day += Duration::days(1);
    }

    Ok(UsageReport {
        totals,
        series,
        by_model,
        retention_days,
    })
}

fn add_bucket(dst: &mut UsageBucket, src: &UsageBucket) {
    dst.requests += src.requests;
    dst.success_requests += src.success_requests;
    dst.prompt_tokens += src.prompt_tokens;
    dst.completion_tokens += src.completion_tokens;
    dst.cache_read_tokens += src.cache_read_tokens;
    dst.cache_write_tokens += src.cache_write_tokens;
    dst.reasoning_tokens += src.reasoning_tokens;
    dst.quota += src.quota;
}

async fn query_daily_by_day(
    pool: &DbPool,
    filter: &LogFilter,
) -> AppResult<Vec<(String, UsageBucket)>> {
    let mut qb = QueryBuilder::new(
        r#"
        SELECT
            day,
            CAST(COALESCE(SUM(requests), 0) AS SIGNED) AS requests,
            CAST(COALESCE(SUM(CASE WHEN status = 'success' THEN requests ELSE 0 END), 0) AS SIGNED) AS success_requests,
            CAST(COALESCE(SUM(prompt_tokens), 0) AS SIGNED) AS prompt_tokens,
            CAST(COALESCE(SUM(completion_tokens), 0) AS SIGNED) AS completion_tokens,
            CAST(COALESCE(SUM(cache_read_tokens), 0) AS SIGNED) AS cache_read_tokens,
            CAST(COALESCE(SUM(cache_write_tokens), 0) AS SIGNED) AS cache_write_tokens,
            CAST(COALESCE(SUM(reasoning_tokens), 0) AS SIGNED) AS reasoning_tokens,
            CAST(COALESCE(SUM(quota), 0) AS SIGNED) AS quota
        FROM log_daily WHERE 1 = 1
        "#,
    );
    filter.push_predicates(&mut qb, "log_daily");
    qb.push(" GROUP BY day ORDER BY day");
    let rows: Vec<(String, i64, i64, i64, i64, i64, i64, i64, i64)> =
        qb.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                day,
                requests,
                success_requests,
                prompt_tokens,
                completion_tokens,
                cache_read_tokens,
                cache_write_tokens,
                reasoning_tokens,
                quota,
            )| {
                (
                    day,
                    UsageBucket {
                        requests,
                        success_requests,
                        prompt_tokens,
                        completion_tokens,
                        cache_read_tokens,
                        cache_write_tokens,
                        reasoning_tokens,
                        quota,
                    },
                )
            },
        )
        .collect())
}

async fn query_hot_by_day(
    pool: &DbPool,
    filter: &LogFilter,
) -> AppResult<Vec<(String, UsageBucket)>> {
    let mut qb = QueryBuilder::new(format!(
        r#"
        SELECT
            {DAY_EXPR} AS day,
            COUNT(*) AS requests,
            CAST(COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) AS SIGNED) AS success_requests,
            CAST(COALESCE(SUM(prompt_tokens), 0) AS SIGNED) AS prompt_tokens,
            CAST(COALESCE(SUM(completion_tokens), 0) AS SIGNED) AS completion_tokens,
            CAST(COALESCE(SUM(cache_read_tokens), 0) AS SIGNED) AS cache_read_tokens,
            CAST(COALESCE(SUM(cache_write_tokens), 0) AS SIGNED) AS cache_write_tokens,
            CAST(COALESCE(SUM(reasoning_tokens), 0) AS SIGNED) AS reasoning_tokens,
            CAST(COALESCE(SUM(quota), 0) AS SIGNED) AS quota
        FROM logs WHERE 1 = 1
        "#
    ));
    filter.push_predicates(&mut qb, "logs");
    qb.push(format!(" GROUP BY {DAY_EXPR} ORDER BY day"));
    let rows: Vec<(String, i64, i64, i64, i64, i64, i64, i64, i64)> =
        qb.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                day,
                requests,
                success_requests,
                prompt_tokens,
                completion_tokens,
                cache_read_tokens,
                cache_write_tokens,
                reasoning_tokens,
                quota,
            )| {
                (
                    day,
                    UsageBucket {
                        requests,
                        success_requests,
                        prompt_tokens,
                        completion_tokens,
                        cache_read_tokens,
                        cache_write_tokens,
                        reasoning_tokens,
                        quota,
                    },
                )
            },
        )
        .collect())
}

async fn query_by_model(pool: &DbPool, filter: &LogFilter) -> AppResult<Vec<UsageByModel>> {
    let mut by_model: std::collections::HashMap<String, UsageByModel> =
        std::collections::HashMap::new();

    let mut qb = QueryBuilder::new(
        r#"
        SELECT
            model,
            CAST(COALESCE(SUM(requests), 0) AS SIGNED) AS requests,
            CAST(COALESCE(SUM(quota), 0) AS SIGNED) AS quota,
            CAST(COALESCE(SUM(prompt_tokens), 0) AS SIGNED) AS prompt_tokens,
            CAST(COALESCE(SUM(completion_tokens), 0) AS SIGNED) AS completion_tokens
        FROM log_daily WHERE 1 = 1
        "#,
    );
    filter.push_predicates(&mut qb, "log_daily");
    qb.push(" GROUP BY model");
    let daily: Vec<UsageByModel> = qb.build_query_as().fetch_all(pool).await?;
    for row in daily {
        merge_model(&mut by_model, row);
    }

    let mut qb = QueryBuilder::new(
        r#"
        SELECT
            model,
            COUNT(*) AS requests,
            CAST(COALESCE(SUM(quota), 0) AS SIGNED) AS quota,
            CAST(COALESCE(SUM(prompt_tokens), 0) AS SIGNED) AS prompt_tokens,
            CAST(COALESCE(SUM(completion_tokens), 0) AS SIGNED) AS completion_tokens
        FROM logs WHERE 1 = 1
        "#,
    );
    filter.push_predicates(&mut qb, "logs");
    qb.push(" GROUP BY model");
    let hot: Vec<UsageByModel> = qb.build_query_as().fetch_all(pool).await?;
    for row in hot {
        merge_model(&mut by_model, row);
    }

    let mut rows: Vec<UsageByModel> = by_model.into_values().collect();
    rows.sort_by(|a, b| b.quota.cmp(&a.quota).then_with(|| b.requests.cmp(&a.requests)));
    rows.truncate(8);
    Ok(rows)
}

fn merge_model(map: &mut std::collections::HashMap<String, UsageByModel>, row: UsageByModel) {
    map.entry(row.model.clone())
        .and_modify(|e| {
            e.requests += row.requests;
            e.quota += row.quota;
            e.prompt_tokens += row.prompt_tokens;
            e.completion_tokens += row.completion_tokens;
        })
        .or_insert(row);
}

fn additive_upsert_clause(engine_name: &str) -> String {
    let cols = [
        "requests",
        "prompt_tokens",
        "completion_tokens",
        "cache_read_tokens",
        "cache_write_tokens",
        "reasoning_tokens",
        "quota",
    ];
    match engine_name {
        "sqlite" => {
            let set = cols
                .iter()
                .map(|c| format!("{c} = {c} + excluded.{c}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "ON CONFLICT(day, user_id, token_id, channel_id, model, status) DO UPDATE SET {set}"
            )
        }
        "mysql" => {
            let set = cols
                .iter()
                .map(|c| format!("{c} = {c} + VALUES({c})"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("ON DUPLICATE KEY UPDATE {set}")
        }
        other => panic!("unsupported engine for log_daily upsert: {other}"),
    }
}

/// Roll expired raw logs into `log_daily` and delete them. Returns total archived rows.
pub async fn archive_expired_logs(pool: &DbPool, retention_days: i64) -> AppResult<u64> {
    if retention_days <= 0 {
        return Ok(0);
    }

    let mut total = 0u64;
    loop {
        let n = archive_batch(pool, retention_days).await?;
        total += n;
        if n < ARCHIVE_BATCH_SIZE as u64 {
            break;
        }
    }
    if total > 0 {
        info!(retention_days, archived = total, "archived expired request logs");
    }
    Ok(total)
}

async fn archive_batch(pool: &DbPool, retention_days: i64) -> AppResult<u64> {
    let engine = db::engine(pool);
    let cutoff = db::time_ago(Duration::days(retention_days));
    let upsert = additive_upsert_clause(engine.name());

    let mut conn = pool.acquire().await?;
    sqlx::query(engine.begin_write())
        .execute(&mut *conn)
        .await?;

    let result = async {
        let ids: Vec<(i64,)> = sqlx::query_as(
            r#"
            SELECT id FROM logs
            WHERE created_at < ?
            ORDER BY id
            LIMIT ?
            "#,
        )
        .bind(&cutoff)
        .bind(ARCHIVE_BATCH_SIZE)
        .fetch_all(&mut *conn)
        .await?;

        if ids.is_empty() {
            return Ok::<u64, AppError>(0);
        }

        let mut id_qb = QueryBuilder::new(format!(
            r#"
            INSERT INTO log_daily (
                day, user_id, token_id, channel_id, model, status,
                requests, prompt_tokens, completion_tokens,
                cache_read_tokens, cache_write_tokens, reasoning_tokens, quota
            )
            SELECT
                {DAY_EXPR},
                user_id,
                COALESCE(token_id, 0),
                COALESCE(channel_id, 0),
                model,
                status,
                COUNT(*),
                CAST(COALESCE(SUM(prompt_tokens), 0) AS SIGNED),
                CAST(COALESCE(SUM(completion_tokens), 0) AS SIGNED),
                CAST(COALESCE(SUM(cache_read_tokens), 0) AS SIGNED),
                CAST(COALESCE(SUM(cache_write_tokens), 0) AS SIGNED),
                CAST(COALESCE(SUM(reasoning_tokens), 0) AS SIGNED),
                CAST(COALESCE(SUM(quota), 0) AS SIGNED)
            FROM logs
            WHERE id IN (
            "#
        ));
        {
            let mut separated = id_qb.separated(", ");
            for (id,) in &ids {
                separated.push_bind(*id);
            }
        }
        id_qb.push(format!(
            r#"
            )
            GROUP BY {DAY_EXPR}, user_id, COALESCE(token_id, 0), COALESCE(channel_id, 0), model, status
            {upsert}
            "#
        ));
        id_qb.build().execute(&mut *conn).await?;

        let mut del_qb = QueryBuilder::new("DELETE FROM logs WHERE id IN (");
        {
            let mut separated = del_qb.separated(", ");
            for (id,) in &ids {
                separated.push_bind(*id);
            }
        }
        del_qb.push(")");
        del_qb.build().execute(&mut *conn).await?;
        Ok(ids.len() as u64)
    }
    .await;

    match result {
        Ok(n) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(n)
        }
        Err(e) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(e)
        }
    }
}

/// Spawn a background task that archives expired logs on startup and every hour (admin only).
pub fn spawn_archive_task(pool: DbPool, retention_days: i64) {
    if retention_days <= 0 {
        info!("log archival disabled (LOG_RETENTION_DAYS=0)");
        return;
    }
    tokio::spawn(async move {
        loop {
            if let Err(e) = archive_expired_logs(&pool, retention_days).await {
                warn!(error = %e, "log archival failed");
            }
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::db;

    async fn test_pool() -> (DbPool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.db");
        let mut cfg = Config::from_env_with_default_port(3000).unwrap();
        cfg.database_url = format!("sqlite://{}?mode=rwc", path.display());
        cfg.log_retention_days = 30;
        (db::connect(&cfg).await.unwrap(), dir)
    }

    #[tokio::test]
    async fn archive_moves_rows_into_daily() {
        let (pool, _dir) = test_pool().await;
        let old = db::time_ago(Duration::days(40));
        sqlx::query(
            r#"
            INSERT INTO logs (user_id, token_id, channel_id, model, prompt_tokens, completion_tokens,
              quota, duration_ms, status, estimated, created_at, cache_read_tokens, cache_write_tokens,
              reasoning_tokens, usage_adapter)
            VALUES (1, 2, 3, 'gpt-4o', 10, 20, 100, 5, 'success', 0, ?,
              1, 2, 3, 'openai_chat')
            "#,
        )
        .bind(&old)
        .execute(&pool)
        .await
        .unwrap();

        let n = archive_expired_logs(&pool, 30).await.unwrap();
        assert_eq!(n, 1);

        let left: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM logs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left.0, 0);

        let daily: (i64, i64, i64) = sqlx::query_as(
            "SELECT requests, prompt_tokens, quota FROM log_daily WHERE model = 'gpt-4o'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(daily, (1, 10, 100));

        let overview = overview(&pool).await.unwrap();
        assert_eq!(overview.total_requests, 1);
        assert_eq!(overview.success_requests, 1);
        assert_eq!(overview.total_quota, 100);
    }

    #[tokio::test]
    async fn usage_merges_hot_and_daily() {
        let (pool, _dir) = test_pool().await;
        let day = db::today();
        sqlx::query(
            r#"
            INSERT INTO log_daily (day, user_id, token_id, channel_id, model, status,
              requests, prompt_tokens, completion_tokens, cache_read_tokens, cache_write_tokens,
              reasoning_tokens, quota)
            VALUES (?, 1, 0, 0, 'gpt-4o', 'success', 2, 20, 40, 0, 0, 0, 200)
            "#,
        )
        .bind(&day)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO logs (user_id, token_id, channel_id, model, prompt_tokens, completion_tokens,
              quota, duration_ms, status, estimated, created_at, cache_read_tokens, cache_write_tokens,
              reasoning_tokens, usage_adapter)
            VALUES (1, NULL, NULL, 'gpt-4o', 5, 5, 50, 1, 'success', 0, ?, 0, 0, 0, '')
            "#,
        )
        .bind(db::now())
        .execute(&pool)
        .await
        .unwrap();

        let report = usage_report(
            &pool,
            &LogFilter {
                start: Some(day.clone()),
                end: Some(day.clone()),
                ..LogFilter::default()
            },
            30,
        )
        .await
        .unwrap();
        assert_eq!(report.totals.requests, 3);
        assert_eq!(report.totals.quota, 250);
        assert_eq!(report.series.len(), 1);
    }
}
