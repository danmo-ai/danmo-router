use crate::config::Config;
use crate::crypto;
use crate::error::{AppError, AppResult};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;
use tracing::info;

pub type DbPool = Pool<Sqlite>;

pub async fn connect(config: &Config) -> AppResult<DbPool> {
    let url = &config.database_url;
    if url.starts_with("postgres") {
        return Err(AppError::BadRequest(
            "Postgres support is planned; use sqlite:// for now".into(),
        ));
    }

    ensure_sqlite_parent_dir(url)?;

    let options = SqliteConnectOptions::from_str(url)
        .map_err(|e| AppError::Other(e.into()))?
        .create_if_missing(true)
        .foreign_keys(true)
        // Multi-process (relay / admin / user) sharing one SQLite file.
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await?;

    migrate(&pool).await?;
    seed_admin(&pool, config).await?;
    seed_defaults(&pool).await?;
    if config.seed_demo {
        crate::demo::seed_demo(&pool, &config.encryption_key).await?;
    }
    Ok(pool)
}

fn ensure_sqlite_parent_dir(url: &str) -> AppResult<()> {
    let Some(rest) = url.strip_prefix("sqlite://") else {
        return Ok(());
    };
    let path = rest.split('?').next().unwrap_or(rest);
    if path.is_empty() || path == ":memory:" || path.starts_with(':') {
        return Ok(());
    }
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| AppError::Other(e.into()))?;
        }
    }
    Ok(())
}

const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/001_init.sql"),
    include_str!("../migrations/002_dialect.sql"),
    include_str!("../migrations/003_channel_models.sql"),
];

/// Runs each migration once, recorded in `schema_migrations`. Databases created before the
/// table existed re-run 001/002 once; those only create-if-missing or add columns.
pub async fn migrate(pool: &DbPool) -> AppResult<()> {
    sqlx::query("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY)")
        .execute(pool)
        .await?;
    for (i, sql) in MIGRATIONS.iter().enumerate() {
        let version = i as i64 + 1;
        let applied: Option<(i64,)> =
            sqlx::query_as("SELECT version FROM schema_migrations WHERE version = ?")
                .bind(version)
                .fetch_optional(pool)
                .await?;
        if applied.is_some() {
            continue;
        }
        let mut tx = pool.begin().await?;
        for statement in sql.split(';') {
            let stmt = statement.trim();
            if stmt.is_empty() {
                continue;
            }
            // SQLite has no `ADD COLUMN IF NOT EXISTS`.
            match sqlx::query(stmt).execute(&mut *tx).await {
                Ok(_) => {}
                Err(sqlx::Error::Database(e)) if e.message().contains("duplicate column name") => {}
                Err(e) => return Err(e.into()),
            }
        }
        sqlx::query("INSERT INTO schema_migrations (version) VALUES (?)")
            .bind(version)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        info!("applied migration {version}");
    }
    Ok(())
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
    sqlx::query(
        r#"
        INSERT INTO users (username, password_hash, role, quota, used_quota, group_name, status)
        VALUES (?, ?, 'admin', 100000000, 0, 'default', 'active')
        "#,
    )
    .bind(&config.admin_username)
    .bind(password_hash)
    .execute(pool)
    .await?;
    info!("seeded admin user '{}'", config.admin_username);
    Ok(())
}

async fn seed_defaults(pool: &DbPool) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO group_ratios (group_name, ratio)
        VALUES ('default', 1.0)
        "#,
    )
    .execute(pool)
    .await?;

    let defaults = [
        ("gpt-4o-mini", 0.15, 0.6),
        ("gpt-4o", 2.5, 1.0),
        ("gpt-3.5-turbo", 0.5, 1.0),
        ("text-embedding-3-small", 0.02, 1.0),
    ];
    for (model, model_ratio, completion_ratio) in defaults {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO model_prices (model, model_ratio, completion_ratio)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(model)
        .bind(model_ratio)
        .bind(completion_ratio)
        .execute(pool)
        .await?;
    }
    Ok(())
}
