use super::SqlEngine;
use sqlx::mysql::MySqlDatabaseError;

pub struct Mysql;

const MIGRATIONS: &[&str] = &[
    include_str!("../../migrations/mysql/001_init.sql"),
    include_str!("../../migrations/mysql/002_dialect.sql"),
    include_str!("../../migrations/mysql/003_channel_models.sql"),
    include_str!("../../migrations/mysql/004_redemptions.sql"),
];

const ER_TABLE_EXISTS: u16 = 1050;
const ER_DUP_FIELDNAME: u16 = 1060;
const ER_DUP_KEYNAME: u16 = 1061;
const ER_CANT_DROP_FIELD_OR_KEY: u16 = 1091;

impl SqlEngine for Mysql {
    fn name(&self) -> &'static str {
        "mysql"
    }

    fn migrations(&self) -> &'static [&'static str] {
        MIGRATIONS
    }

    /// Reject over-long values instead of truncating them.
    fn session_setup(&self) -> &'static [&'static str] {
        &["SET SESSION sql_mode = CONCAT(@@sql_mode, ',STRICT_TRANS_TABLES')"]
    }

    fn begin_write(&self) -> &'static str {
        "BEGIN"
    }

    fn lock_rows(&self) -> &'static str {
        " FOR UPDATE"
    }

    fn returning_id(&self) -> &'static str {
        ""
    }

    fn insert_ignore(&self) -> &'static str {
        "INSERT IGNORE"
    }

    fn upsert(&self, _key: &str, cols: &[&str]) -> String {
        let set: Vec<String> = cols.iter().map(|c| format!("{c} = VALUES({c})")).collect();
        format!("ON DUPLICATE KEY UPDATE {}", set.join(", "))
    }

    fn migration_lock(&self) -> Option<(&'static str, &'static str)> {
        Some((
            "SELECT GET_LOCK('danmo_migrate', 60)",
            "SELECT RELEASE_LOCK('danmo_migrate')",
        ))
    }

    fn already_applied(&self, err: &sqlx::Error) -> bool {
        let sqlx::Error::Database(e) = err else {
            return false;
        };
        e.try_downcast_ref::<MySqlDatabaseError>().is_some_and(|e| {
            matches!(
                e.number(),
                ER_TABLE_EXISTS | ER_DUP_FIELDNAME | ER_DUP_KEYNAME | ER_CANT_DROP_FIELD_OR_KEY
            )
        })
    }
}
