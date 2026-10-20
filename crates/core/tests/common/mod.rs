#![allow(dead_code)]

use sqlx::Connection;
use tempfile::TempDir;

/// Server root URL such as `mysql://user:pass@127.0.0.1:3306`. When set, every test gets its
/// own database on that server instead of a SQLite file.
pub const MYSQL_ENV: &str = "DANMO_TEST_MYSQL_URL";

pub struct TestDb {
    pub url: String,
    _dir: TempDir,
}

impl TestDb {
    pub fn is_mysql(&self) -> bool {
        self.url.starts_with("mysql")
    }
}

pub async fn test_db() -> TestDb {
    let dir = tempfile::tempdir().unwrap();
    let url = match std::env::var(MYSQL_ENV) {
        Ok(server) if !server.trim().is_empty() => mysql_database(server.trim()).await,
        _ => format!("sqlite://{}?mode=rwc", dir.path().join("test.db").display()),
    };
    TestDb { url, _dir: dir }
}

async fn mysql_database(server: &str) -> String {
    let server = server.trim_end_matches('/');
    let name = format!("danmo_test_{}", uuid::Uuid::new_v4().simple());
    let mut conn = sqlx::MySqlConnection::connect(server).await.unwrap();
    sqlx::query(&format!(
        "CREATE DATABASE {name} CHARACTER SET utf8mb4 COLLATE utf8mb4_bin"
    ))
    .execute(&mut conn)
    .await
    .unwrap();
    format!("{server}/{name}")
}
