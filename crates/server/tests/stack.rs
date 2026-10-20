//! Admin, user, and relay HTTP on one fresh database and one mock upstream.
//!
//! Same router merge as the `danmo-router` binary. Each test gets its own file.

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::Router;
use chrono::{NaiveTime, Timelike, Utc};
use danmo_admin_api::AdminState;
use danmo_core::billing::{estimate_pre_consume, PricePeriod, PriceRule, PriceTier};
use danmo_core::config::Config;
use danmo_core::db::connect;
use danmo_core::usage::Usage;
use danmo_relay::RelayState;
use danmo_user_api::UserState;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;
use wiremock::matchers::{header, method};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct Stack {
    app: Router,
    mock: MockServer,
    admin_cookie: String,
    user_cookie: String,
    key: String,
    _dir: TempDir,
}

async fn boot() -> (Router, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::from_env().unwrap();
    config.database_url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("stack.db").display()
    );
    config.seed_demo = false;
    config.admin_username = "admin".into();
    config.admin_password = "admin123".into();
    config.relay_max_retries = 2;
    config.relay_user_concurrency = 1_000;
    config.relay_instance_concurrency = 1_000;
    config.relay_user_qps = 1_000;
    config.relay_instance_qps = 1_000;
    config.relay_legacy_concurrency = 1_000;
    config.relay_legacy_qps = 1_000;
    let config = Arc::new(config);
    let pool = connect(&config).await.unwrap();
    let app = Router::new()
        .merge(danmo_relay::router(RelayState::new(
            pool.clone(),
            config.clone(),
        )))
        .merge(danmo_admin_api::router(AdminState::new(
            pool.clone(),
            config.clone(),
        )))
        .merge(danmo_user_api::router(UserState::new(pool, config)));
    (app, dir)
}

fn session_cookie(headers: &HeaderMap) -> String {
    let raw = headers
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .expect("login sets a cookie");
    let pair = raw.split(';').next().unwrap();
    assert!(pair.starts_with("danmo_session="), "{raw}");
    pair.to_string()
}

async fn call(
    app: &Router,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    bearer: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, HeaderMap, Value) {
    let mut req = Request::builder().method(method).uri(path);
    if let Some(cookie) = cookie {
        req = req.header("cookie", cookie);
    }
    if let Some(bearer) = bearer {
        req = req.header("authorization", format!("Bearer {bearer}"));
    }
    let req = match body {
        Some(v) => req
            .header("content-type", "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("{status} not json: {}", String::from_utf8_lossy(&bytes)));
    (status, headers, json)
}

impl Stack {
    /// Admin session, one user in `group`, and that user's token.
    async fn open(group: &str) -> Self {
        let (app, dir) = boot().await;
        let mock = MockServer::start().await;
        let (status, headers, _) = call(
            &app,
            "POST",
            "/api/admin/login",
            None,
            None,
            Some(json!({"username": "admin", "password": "admin123"})),
        )
        .await;
        assert_eq!(status, 200);
        let admin_cookie = session_cookie(&headers);
        let (status, _, created) = call(
            &app,
            "POST",
            "/api/admin/users",
            Some(&admin_cookie),
            None,
            Some(json!({
                "username": "alice",
                "password": "alice123",
                "quota": 1_000_000,
                "group_name": group
            })),
        )
        .await;
        assert_eq!(status, 200, "{created}");
        let (status, headers, _) = call(
            &app,
            "POST",
            "/api/user/login",
            None,
            None,
            Some(json!({"username": "alice", "password": "alice123"})),
        )
        .await;
        assert_eq!(status, 200);
        let user_cookie = session_cookie(&headers);
        let (status, _, token) = call(
            &app,
            "POST",
            "/api/user/tokens",
            Some(&user_cookie),
            None,
            Some(json!({"name": "primary", "unlimited_quota": true})),
        )
        .await;
        assert_eq!(status, 200, "{token}");
        let key = token["data"]["key"].as_str().unwrap().to_string();
        Self {
            app,
            mock,
            admin_cookie,
            user_cookie,
            key,
            _dir: dir,
        }
    }

    async fn admin(&self, method: &str, path: &str, body: Value) -> Value {
        let (status, _, json) = call(
            &self.app,
            method,
            path,
            Some(&self.admin_cookie),
            None,
            Some(body),
        )
        .await;
        assert_eq!(status, 200, "{json}");
        assert_eq!(json["success"], true, "{json}");
        json
    }

    async fn user_get(&self, path: &str) -> Value {
        let (status, _, json) =
            call(&self.app, "GET", path, Some(&self.user_cookie), None, None).await;
        assert_eq!(status, 200, "{json}");
        json
    }

    fn used_quota_of(me: &Value) -> i64 {
        me["data"]["used_quota"].as_i64().unwrap()
    }

    async fn used_quota(&self) -> i64 {
        Self::used_quota_of(&self.user_get("/api/user/me").await)
    }

    async fn relay(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let (status, _, json) = call(&self.app, method, path, None, Some(&self.key), body).await;
        (status, json)
    }

    async fn channel(&self, body: Value) -> i64 {
        let created = self.admin("POST", "/api/admin/channels", body).await;
        created["data"]["id"].as_i64().unwrap()
    }

    fn base(&self) -> String {
        self.mock.uri()
    }
}

fn chat_usage(prompt: i64, cached: i64, completion: i64) -> Value {
    json!({
        "id": "chatcmpl-mock",
        "object": "chat.completion",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "ok"},
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": prompt,
            "completion_tokens": completion,
            "total_tokens": prompt + completion,
            "prompt_tokens_details": {"cached_tokens": cached}
        }
    })
}

struct Chan<'a> {
    name: &'a str,
    key: &'a str,
    base: &'a str,
    model: &'a str,
    upstream_model: &'a str,
    dialect: &'a str,
    group: &'a str,
    priority: i64,
    relay_mode: &'a str,
    protocol: &'a str,
    auth: &'a str,
}

impl<'a> Chan<'a> {
    fn openai(name: &'a str, key: &'a str, base: &'a str, model: &'a str) -> Self {
        Self {
            name,
            key,
            base,
            model,
            upstream_model: "",
            dialect: "",
            group: "default",
            priority: 1,
            relay_mode: "passthrough",
            protocol: "openai_chat",
            auth: "bearer",
        }
    }

    fn json(&self) -> Value {
        json!({
            "name": self.name,
            "key": self.key,
            "base_url": self.base,
            "models": [{
                "model": self.model,
                "upstream_model": self.upstream_model,
                "upstream_dialect": self.dialect
            }],
            "group_name": self.group,
            "priority": self.priority,
            "relay_mode": self.relay_mode,
            "upstream_protocol": self.protocol,
            "auth_style": self.auth,
            "default_dialect": self.dialect
        })
    }
}

async fn mount_bearer(mock: &MockServer, key: &str, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(header("authorization", format!("Bearer {key}")))
        .respond_with(response)
        .mount(mock)
        .await;
}

fn auths(reqs: &[wiremock::Request]) -> Vec<String> {
    reqs.iter()
        .map(|r| {
            r.headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string()
        })
        .collect()
}

#[tokio::test]
async fn passthrough_remaps_model_and_agrees_on_quota() {
    let s = Stack::open("default").await;
    let base = s.base();
    mount_bearer(
        &s.mock,
        "mock-ok",
        ResponseTemplate::new(200).set_body_json(chat_usage(11, 0, 7)),
    )
    .await;
    s.admin(
        "PUT",
        "/api/admin/model-prices",
        json!({
            "model": "gpt-stack",
            "model_ratio": 1.0,
            "completion_ratio": 1.0
        }),
    )
    .await;
    let mut gpt = Chan::openai("openai", "mock-ok", &base, "gpt-stack");
    gpt.upstream_model = "gpt-stack-upstream";
    s.channel(gpt.json()).await;

    let body = json!({
        "model": "gpt-stack",
        "messages": [{"role": "user", "content": "hi"}],
        "temperature": 0.7,
        "vendor_specific_field": {"weird": ["keep", 123]}
    });
    let (status, reply) = s
        .relay("POST", "/v1/chat/completions", Some(body.clone()))
        .await;
    assert_eq!(status, 200, "{reply}");
    assert_eq!(reply["usage"]["prompt_tokens"], 11);
    assert_eq!(reply["usage"]["completion_tokens"], 7);

    let sent: Value = s
        .mock
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .next()
        .unwrap();
    let mut expect = body;
    expect["model"] = json!("gpt-stack-upstream");
    assert_eq!(sent, expect);

    let (status, models) = s.relay("GET", "/v1/models", None).await;
    assert_eq!(status, 200, "{models}");
    let ids: Vec<&str> = models["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["gpt-stack"]);

    assert_eq!(s.used_quota().await, 18);
    let logs = s.user_get("/api/user/logs?status=success").await;
    assert_eq!(logs["data"][0]["quota"], 18);
    assert_eq!(logs["data"][0]["model"], "gpt-stack");
}

fn tiered_rule(periods: Vec<PricePeriod>) -> PriceRule {
    PriceRule {
        model_ratio: 0.27,
        completion_ratio: 1.5,
        cache_read_ratio: 0.1,
        tiers: vec![PriceTier {
            max_input: Some(32_000),
            model_ratio: 0.14,
            ..PriceTier::default()
        }],
        periods,
        ..PriceRule::default()
    }
}

fn sample_usage() -> Usage {
    Usage {
        input_tokens: 100,
        cache_read_tokens: 40,
        output_tokens: 20,
        ..Usage::default()
    }
}

fn minute_now() -> NaiveTime {
    let now = Utc::now();
    NaiveTime::from_hms_opt(now.hour(), now.minute(), 0).unwrap()
}

#[tokio::test]
async fn billing_settles_to_the_price_rule_and_refunds_without_usage() {
    let s = Stack::open("vip").await;
    let base = s.base();
    mount_bearer(
        &s.mock,
        "mock-ok",
        ResponseTemplate::new(200).set_body_json(chat_usage(100, 40, 20)),
    )
    .await;
    mount_bearer(
        &s.mock,
        "mock-nousage",
        ResponseTemplate::new(200).set_body_json(json!({
            "id": "x",
            "choices": [{"index": 0, "message": {"content": "free?"}}]
        })),
    )
    .await;
    s.admin(
        "PUT",
        "/api/admin/group-ratios",
        json!({"group_name": "vip", "ratio": 0.8}),
    )
    .await;
    let price = json!({
        "model": "tiered",
        "model_ratio": 0.27,
        "completion_ratio": 1.5,
        "cache_read_ratio": 0.1,
        "cache_write_ratio": 1.0,
        "tiers": r#"[{"max_input":32000,"model_ratio":0.14}]"#
    });
    s.admin("PUT", "/api/admin/model-prices", price.clone())
        .await;
    let mut chan = Chan::openai("openai", "mock-ok", &base, "tiered");
    chan.group = "vip";
    let id = s.channel(chan.json()).await;

    let before = s.used_quota().await;
    let (status, reply) = s
        .relay(
            "POST",
            "/v1/chat/completions",
            Some(json!({"model": "tiered", "messages": [], "max_tokens": 50000})),
        )
        .await;
    assert_eq!(status, 200, "{reply}");
    let expected = tiered_rule(vec![]).quota(&sample_usage(), 0.8, minute_now());
    let settled = s.used_quota().await - before;
    assert_eq!(settled, expected);
    assert!(settled < estimate_pre_consume(Some(50_000), 0.27, 0.8));

    let mut discounted = price;
    discounted["periods"] = json!(
        r#"[{"start":"00:00","end":"23:59","multiplier":0.5},{"start":"23:59","end":"00:00","multiplier":0.5}]"#
    );
    s.admin("PUT", "/api/admin/model-prices", discounted).await;
    let before = s.used_quota().await;
    let (status, reply) = s
        .relay(
            "POST",
            "/v1/chat/completions",
            Some(json!({"model": "tiered", "messages": []})),
        )
        .await;
    assert_eq!(status, 200, "{reply}");
    let periods = vec![
        PricePeriod {
            start: "00:00".into(),
            end: "23:59".into(),
            multiplier: 0.5,
        },
        PricePeriod {
            start: "23:59".into(),
            end: "00:00".into(),
            multiplier: 0.5,
        },
    ];
    assert_eq!(
        s.used_quota().await - before,
        tiered_rule(periods).quota(&sample_usage(), 0.8, minute_now())
    );

    chan.key = "mock-nousage";
    s.admin("PUT", &format!("/api/admin/channels/{id}"), chan.json())
        .await;
    let before = s.used_quota().await;
    let (status, reply) = s
        .relay(
            "POST",
            "/v1/chat/completions",
            Some(json!({"model": "tiered", "messages": []})),
        )
        .await;
    assert_eq!(status, 502, "{reply}");
    assert_eq!(reply["error"]["type"], "billing_failed");
    assert_eq!(s.used_quota().await, before);
    let logs = s.user_get("/api/user/logs?status=billing_failed").await;
    assert_eq!(logs["data"][0]["model"], "tiered");
    assert_eq!(logs["data"][0]["quota"], 0);
}

#[tokio::test]
async fn routing_follows_protocol_group_and_convert_mode() {
    let s = Stack::open("default").await;
    let base = s.base();
    mount_bearer(
        &s.mock,
        "mock-ok",
        ResponseTemplate::new(200).set_body_json(json!({
            "id": "c",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "ok",
                    "reasoning_content": "mock-reasoning"
                },
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        })),
    )
    .await;
    s.admin(
        "PUT",
        "/api/admin/model-prices",
        json!({
            "model": "qwen-stack",
            "model_ratio": 1.0,
            "completion_ratio": 1.0,
            "native_dialects": {"openai_chat": "enable_thinking"}
        }),
    )
    .await;
    let mut claude = Chan::openai("claude", "mock-ok", &base, "claude-stack");
    claude.protocol = "anthropic_messages";
    claude.auth = "x_api_key";
    s.channel(claude.json()).await;
    let mut secret = Chan::openai("secret", "mock-ok", &base, "secret-model");
    secret.group = "vip";
    s.channel(secret.json()).await;
    let mut qwen = Chan::openai("qwen", "mock-ok", &base, "qwen-stack");
    qwen.dialect = "enable_thinking";
    qwen.relay_mode = "convert";
    s.channel(qwen.json()).await;

    let (status, models) = s.relay("GET", "/v1/models", None).await;
    assert_eq!(status, 200, "{models}");
    let ids: Vec<&str> = models["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"qwen-stack"), "{ids:?}");
    assert!(ids.contains(&"claude-stack"), "{ids:?}");
    assert!(!ids.contains(&"secret-model"), "{ids:?}");

    let before = s.used_quota().await;
    let seen = s.mock.received_requests().await.unwrap().len();
    let (status, reply) = s
        .relay(
            "POST",
            "/v1/chat/completions",
            Some(json!({"model": "claude-stack", "messages": []})),
        )
        .await;
    assert_eq!(status, 502, "{reply}");
    assert!(reply["error"]["message"]
        .as_str()
        .unwrap()
        .contains("no available openai_chat channel"));
    assert_eq!(s.used_quota().await, before);
    assert_eq!(s.mock.received_requests().await.unwrap().len(), seen);

    let (status, reply) = s
        .relay(
            "POST",
            "/v1/chat/completions",
            Some(json!({
                "model": "qwen-stack",
                "messages": [{"role": "user", "content": "hi"}],
                "enable_thinking": true
            })),
        )
        .await;
    assert_eq!(status, 200, "{reply}");
    assert_eq!(
        reply["choices"][0]["message"]["reasoning_content"],
        "mock-reasoning"
    );
    let sent: Value = serde_json::from_slice(
        &s.mock
            .received_requests()
            .await
            .unwrap()
            .last()
            .unwrap()
            .body,
    )
    .unwrap();
    assert_eq!(sent["enable_thinking"], true);
}

#[tokio::test]
async fn failover_uses_the_backup_and_keeps_a_rate_limited_channel_out() {
    let s = Stack::open("default").await;
    let base = s.base();
    mount_bearer(
        &s.mock,
        "mock-ok",
        ResponseTemplate::new(200).set_body_json(chat_usage(3, 0, 1)),
    )
    .await;
    mount_bearer(
        &s.mock,
        "mock-500",
        ResponseTemplate::new(500).set_body_json(json!({"error": "down"})),
    )
    .await;
    mount_bearer(
        &s.mock,
        "mock-429",
        ResponseTemplate::new(429)
            .insert_header("retry-after", "60")
            .set_body_json(json!({"error": "slow"})),
    )
    .await;
    let mut primary = Chan::openai("primary", "mock-500", &base, "m");
    primary.priority = 10;
    let id = s.channel(primary.json()).await;
    s.channel(Chan::openai("backup", "mock-ok", &base, "m").json())
        .await;

    let chat = json!({"model": "m", "messages": []});
    let (status, reply) = s
        .relay("POST", "/v1/chat/completions", Some(chat.clone()))
        .await;
    assert_eq!(status, 200, "{reply}");
    assert_eq!(
        auths(&s.mock.received_requests().await.unwrap()),
        ["Bearer mock-500", "Bearer mock-ok"]
    );

    primary.key = "mock-429";
    s.admin("PUT", &format!("/api/admin/channels/{id}"), primary.json())
        .await;
    let (status, reply) = s
        .relay("POST", "/v1/chat/completions", Some(chat.clone()))
        .await;
    assert_eq!(status, 200, "{reply}");

    let before = s.mock.received_requests().await.unwrap().len();
    let (status, reply) = s.relay("POST", "/v1/chat/completions", Some(chat)).await;
    assert_eq!(status, 200, "{reply}");
    let after = s.mock.received_requests().await.unwrap();
    assert_eq!(after.len(), before + 1);
    assert_eq!(
        after.last().unwrap().headers.get("authorization").unwrap(),
        "Bearer mock-ok"
    );
}
