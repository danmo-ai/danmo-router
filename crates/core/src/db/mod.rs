//! One shared SQL code path over `sqlx::Any`. The few statements that differ between database
//! engines live behind [`SqlEngine`]; each engine implements only those.

use crate::config::Config;
use crate::crypto;
use crate::error::{AppError, AppResult};
use sqlx::any::{AnyArguments, AnyPoolOptions};
use sqlx::query::Query;
use sqlx::{Any, AnyConnection, AnyPool, Connection, Executor, Row, Transaction};
use std::time::Duration;
use tracing::info;

mod mysql;
mod sqlite;

pub type DbPool = AnyPool;
pub type DbTx = Transaction<'static, Any>;

/// Stored timestamp format, UTC. Same text as SQLite `datetime('now')`, so it sorts and
/// compares as a string on every engine.
pub const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

pub trait SqlEngine: Send + Sync {
    fn name(&self) -> &'static str;

    /// Migration files in version order; position + 1 is the version in `schema_migrations`.
    fn migrations(&self) -> &'static [&'static str];

    /// Runs before the first connection, e.g. to create the SQLite file's directory.
    fn prepare(&self, _url: &str) -> AppResult<()> {
        Ok(())
    }

    /// Runs on every new connection.
    fn session_setup(&self) -> &'static [&'static str] {
        &[]
    }

    /// Opens a transaction that will write.
    fn begin_write(&self) -> &'static str;

    /// Appended to a `SELECT` inside a write transaction whose rows decide what it writes.
    fn lock_rows(&self) -> &'static str;

    /// Appended to an `INSERT` so it yields the new `id` as a row; empty when the engine
    /// reports it in the query result instead. Use with [`insert_id`].
    fn returning_id(&self) -> &'static str;

    /// `INSERT` keyword that skips rows violating a unique key.
    fn insert_ignore(&self) -> &'static str;

    /// Clause after `INSERT ... VALUES (...)` that overwrites `cols` when unique `key` exists.
    fn upsert(&self, key: &str, cols: &[&str]) -> String;

    /// Acquire and release statements held across `migrate`, for engines whose DDL commits
    /// implicitly and so cannot be serialized by the migration transaction.
    fn migration_lock(&self) -> Option<(&'static str, &'static str)> {
        None
    }

    /// A migration statement failed because an earlier, interrupted run already applied it.
    fn already_applied(&self, err: &sqlx::Error) -> bool;
}

pub fn engine_for_url(url: &str) -> AppResult<&'static dyn SqlEngine> {
    match url.split(':').next().unwrap_or_default() {
        "sqlite" => Ok(&sqlite::Sqlite),
        "mysql" | "mariadb" => Ok(&mysql::Mysql),
        "postgres" | "postgresql" => Err(AppError::BadRequest(
            "Postgres support is planned; use sqlite:// or mysql://".into(),
        )),
        other => Err(AppError::BadRequest(format!(
            "unsupported DATABASE_URL scheme '{other}'; use sqlite:// or mysql://"
        ))),
    }
}

pub fn engine(pool: &DbPool) -> &'static dyn SqlEngine {
    engine_for_url(pool.connect_options().database_url.as_str())
        .expect("pools are only built from supported URLs")
}

pub fn now() -> String {
    chrono::Utc::now().format(TIME_FORMAT).to_string()
}

pub fn time_ago(ago: chrono::Duration) -> String {
    (chrono::Utc::now() - ago).format(TIME_FORMAT).to_string()
}

/// Start of the current UTC day, comparable with stored timestamps.
pub fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

/// Runs an `INSERT` whose SQL ends with [`SqlEngine::returning_id`] and returns the new id.
pub async fn insert_id<'q, 'c, E>(
    engine: &dyn SqlEngine,
    query: Query<'q, Any, AnyArguments<'q>>,
    executor: E,
) -> Result<i64, sqlx::Error>
where
    'q: 'c,
    E: Executor<'c, Database = Any> + 'c,
{
    if engine.returning_id().is_empty() {
        query
            .execute(executor)
            .await?
            .last_insert_id()
            .ok_or_else(|| sqlx::Error::Protocol("database returned no inserted id".into()))
    } else {
        query.fetch_one(executor).await?.try_get(0)
    }
}

/// On SQLite this takes the write lock up front: a deferred transaction that reads and then
/// writes fails with `SQLITE_BUSY` / `SQLITE_BUSY_SNAPSHOT` at the upgrade without honoring
/// `busy_timeout`.
pub async fn begin_write(pool: &DbPool) -> AppResult<DbTx> {
    Ok(pool.begin_with(engine(pool).begin_write()).await?)
}

pub async fn connect(config: &Config) -> AppResult<DbPool> {
    let url = &config.database_url;
    let engine = engine_for_url(url)?;
    engine.prepare(url)?;
    sqlx::any::install_default_drivers();

    let pool = AnyPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .after_connect(move |conn, _| {
            Box::pin(async move {
                for sql in engine.session_setup() {
                    sqlx::query(sql).execute(&mut *conn).await?;
                }
                Ok(())
            })
        })
        .connect(url)
        .await?;

    migrate(&pool).await?;
    seed_admin(&pool, config).await?;
    seed_defaults(&pool).await?;
    if config.seed_demo {
        crate::demo::seed_demo(&pool, &config.encryption_key).await?;
    }
    info!("connected to {} database", engine.name());
    Ok(pool)
}

/// Runs each migration once, recorded in `schema_migrations`. Databases created before the
/// table existed re-run 001/002 once; those only create-if-missing or add columns.
pub async fn migrate(pool: &DbPool) -> AppResult<()> {
    let engine = engine(pool);
    let mut conn = pool.acquire().await?;
    if let Some((lock, _)) = engine.migration_lock() {
        let (got,): (i64,) = sqlx::query_as(lock).fetch_one(&mut *conn).await?;
        if got != 1 {
            return Err(AppError::Other(anyhow::anyhow!(
                "timed out waiting for another process to finish migrations"
            )));
        }
    }
    let result = apply_migrations(&mut conn, engine).await;
    if let Some((_, unlock)) = engine.migration_lock() {
        sqlx::query(unlock).execute(&mut *conn).await?;
    }
    result
}

async fn apply_migrations(conn: &mut AnyConnection, engine: &dyn SqlEngine) -> AppResult<()> {
    sqlx::query("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY)")
        .execute(&mut *conn)
        .await?;
    for (i, sql) in engine.migrations().iter().enumerate() {
        let version = i as i64 + 1;
        if is_applied(&mut *conn, version).await? {
            continue;
        }
        let mut tx = conn.begin_with(engine.begin_write()).await?;
        // Another process may have applied it while this one waited for the write lock.
        if is_applied(&mut tx, version).await? {
            continue;
        }
        for statement in sql.split(';') {
            let stmt = statement.trim();
            if stmt.is_empty() {
                continue;
            }
            match sqlx::query(stmt).execute(&mut *tx).await {
                Ok(_) => {}
                Err(e) if engine.already_applied(&e) => {}
                Err(e) => return Err(e.into()),
            }
        }
        sqlx::query("INSERT INTO schema_migrations (version) VALUES (?)")
            .bind(version)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        info!("applied {} migration {version}", engine.name());
    }
    Ok(())
}

async fn is_applied(conn: &mut AnyConnection, version: i64) -> AppResult<bool> {
    let applied: Option<(i64,)> =
        sqlx::query_as("SELECT version FROM schema_migrations WHERE version = ?")
            .bind(version)
            .fetch_optional(conn)
            .await?;
    Ok(applied.is_some())
}

async fn seed_admin(pool: &DbPool, config: &Config) -> AppResult<()> {
    let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE username = ?")
        .bind(&config.admin_username)
        .fetch_optional(pool)
        .await?;
    if exists.is_some() {
        return Ok(());
    }

    let password_hash = crypto::hash_password(&config.admin_password).map_err(AppError::Other)?;
    let now = now();
    let inserted = sqlx::query(
        r#"
        INSERT INTO users
            (username, password_hash, role, quota, used_quota, group_name, status, created_at, updated_at)
        VALUES (?, ?, 'admin', 100000000, 0, 'default', 'active', ?, ?)
        "#,
    )
    .bind(&config.admin_username)
    .bind(password_hash)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await;
    match inserted {
        Ok(_) => info!("seeded admin user '{}'", config.admin_username),
        // Another process (admin / user / relay) seeded the same username first.
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

async fn seed_defaults(pool: &DbPool) -> AppResult<()> {
    let insert = engine(pool).insert_ignore();
    let now = now();
    sqlx::query(&format!(
        "{insert} INTO group_ratios (group_name, ratio, updated_at) VALUES ('default', 1.0, ?)"
    ))
    .bind(&now)
    .execute(pool)
    .await?;

    let defaults = [
        ("gpt-4o-mini", 0.15, 0.6),
        ("gpt-4o", 2.5, 1.0),
        ("gpt-3.5-turbo", 0.5, 1.0),
        ("text-embedding-3-small", 0.02, 1.0),
    ];
    let sql = format!(
        "{insert} INTO model_prices (model, model_ratio, completion_ratio, updated_at) VALUES (?, ?, ?, ?)"
    );
    for (model, model_ratio, completion_ratio) in defaults {
        sqlx::query(&sql)
            .bind(model)
            .bind(model_ratio)
            .bind(completion_ratio)
            .bind(&now)
            .execute(pool)
            .await?;
    }
    Ok(())
}
