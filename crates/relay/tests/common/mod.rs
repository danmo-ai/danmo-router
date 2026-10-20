#![allow(dead_code)]

use axum::http::{HeaderMap, StatusCode};
use bytes::Bytes;
use danmo_core::config::Config;
use danmo_core::db::connect;
use danmo_core::models::LogEntry;
use danmo_core::services::{channel, log as log_svc, token as token_svc, user as user_svc};
use danmo_core::DbPool;
use danmo_relay::RelayState;
use http_body_util::BodyExt;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tower::ServiceExt;
use wiremock::MockServer;

pub const USER_QUOTA: i64 = 10_000_000;

pub struct Harness {
    pub pool: DbPool,
    pub config: Arc<Config>,
    pub mock: MockServer,
    pub key: String,
    pub user_id: i64,
    /// Shared across calls so channel health carries over between requests.
    state: RelayState,
    _dir: TempDir,
}

impl Harness {
    pub async fn new() -> Self {
        Self::with_config(|_| {}).await
    }

    pub async fn with_config(edit: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("relay.db");
        let mut config = Config::from_env().unwrap();
        config.database_url = format!("sqlite://{}?mode=rwc", db_path.display());
        config.relay_max_retries = 2;
        edit(&mut config);
        let config = Arc::new(config);
        let pool = connect(&config).await.unwrap();
        let user = user_svc::create_user(&pool, "u1", "secret1", "user", USER_QUOTA, "default")
            .await
            .unwrap();
        let created = token_svc::create_token(&pool, user.id, "tok", -1, true, None, "", None)
            .await
            .unwrap();
        Self {
            state: RelayState::new(pool.clone(), config.clone()),
            pool,
            config,
            mock: MockServer::start().await,
            key: created.raw_key,
            user_id: user.id,
            _dir: dir,
        }
    }

    pub async fn channel(&self, input: channel::ChannelInput) {
        let input = channel::ChannelInput {
            key: "sk-upstream".into(),
            base_url: if input.base_url.is_empty() {
                self.mock.uri()
            } else {
                input.base_url
            },
            ..input
        };
        channel::create_channel(&self.pool, &self.config.encryption_key, input)
            .await
            .unwrap();
    }

    pub async fn call(
        &self,
        uri: &str,
        headers: &[(&str, String)],
        body: impl Into<String>,
    ) -> (StatusCode, HeaderMap, Bytes) {
        let app = danmo_relay::router(self.state.clone());
        let mut req = axum::http::Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json");
        for (k, v) in headers {
            req = req.header(*k, v);
        }
        let resp = app
            .oneshot(req.body(axum::body::Body::from(body.into())).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, headers, bytes)
    }

    pub fn bearer(&self) -> Vec<(&'static str, String)> {
        vec![("authorization", format!("Bearer {}", self.key))]
    }

    /// Stream settlement runs after the body ends, so poll for the log row.
    pub async fn last_log(&self) -> LogEntry {
        for _ in 0..100 {
            let logs = log_svc::list_logs(&self.pool, Some(self.user_id), 1, 0)
                .await
                .unwrap();
            if let Some(l) = logs.into_iter().next() {
                return l;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("no log written");
    }

    pub async fn used_quota(&self) -> i64 {
        user_svc::get_by_id(&self.pool, self.user_id)
            .await
            .unwrap()
            .used_quota
    }

    pub async fn upstream_requests(&self) -> Vec<wiremock::Request> {
        self.mock.received_requests().await.unwrap()
    }
}

pub fn sse(events: &[&str]) -> String {
    events.iter().map(|e| format!("{e}\n\n")).collect()
}

pub fn data_lines(body: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(body)
        .lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter(|d| *d != "[DONE]")
        .map(|d| serde_json::from_str(d).unwrap())
        .collect()
}
