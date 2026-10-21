use super::SqlEngine;
use crate::error::{AppError, AppResult};
use std::path::Path;

pub struct Sqlite;

const MIGRATIONS: &[&str] = &[
    include_str!("../../migrations/sqlite/001_init.sql"),
    include_str!("../../migrations/sqlite/002_dialect.sql"),
    include_str!("../../migrations/sqlite/003_channel_models.sql"),
    include_str!("../../migrations/sqlite/004_redemptions.sql"),
    include_str!("../../migrations/sqlite/005_log_daily.sql"),
];

impl SqlEngine for Sqlite {
    fn name(&self) -> &'static str {
        "sqlite"
    }

    fn migrations(&self) -> &'static [&'static str] {
        MIGRATIONS
    }

    fn prepare(&self, url: &str) -> AppResult<()> {
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

    /// WAL lets relay / admin / user processes share one file.
    fn session_setup(&self) -> &'static [&'static str] {
        &[
            "PRAGMA busy_timeout = 5000",
            "PRAGMA journal_mode = WAL",
            "PRAGMA foreign_keys = ON",
        ]
    }

    fn begin_write(&self) -> &'static str {
        "BEGIN IMMEDIATE"
    }

    /// `BEGIN IMMEDIATE` already holds the database write lock.
    fn lock_rows(&self) -> &'static str {
        ""
    }

    /// The Any driver does not pass SQLite's last rowid through.
    fn returning_id(&self) -> &'static str {
        " RETURNING id"
    }

    fn insert_ignore(&self) -> &'static str {
        "INSERT OR IGNORE"
    }

    fn upsert(&self, key: &str, cols: &[&str]) -> String {
        let set: Vec<String> = cols.iter().map(|c| format!("{c} = excluded.{c}")).collect();
        format!("ON CONFLICT({key}) DO UPDATE SET {}", set.join(", "))
    }

    /// SQLite has no `ADD COLUMN IF NOT EXISTS`.
    fn already_applied(&self, err: &sqlx::Error) -> bool {
        matches!(err, sqlx::Error::Database(e) if e.message().contains("duplicate column name"))
    }
}
