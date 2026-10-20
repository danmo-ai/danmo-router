mod common;

use common::Harness;
use danmo_core::config::Config;
use danmo_core::crypto;
use danmo_core::services::channel::ChannelInput;
use danmo_core::services::{token as token_svc, user as user_svc};
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

fn chat_body() -> String {
    r#"{"model":"gpt-4o-mini","messages":[{"role":"user","content":"hi"}]}"#.into()
}

fn limits(tune: impl FnOnce(&mut Config)) -> impl FnOnce(&mut Config) {
    move |config: &mut Config| {
        config.relay_user_concurrency = 8;
        config.relay_instance_concurrency = 8;
        config.relay_user_qps = 50;
        config.relay_instance_qps = 50;
        config.relay_legacy_concurrency = 4;
        config.relay_legacy_qps = 20;
        tune(config);
    }
}

async fn ready(h: &Harness, delay: Option<Duration>) {
    let mut response = ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "id": "chatcmpl-1",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
    }));
    if let Some(delay) = delay {
        response = response.set_delay(delay);
    }
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(response)
        .mount(&h.mock)
        .await;
    h.channel(ChannelInput {
        models: danmo_core::services::channel::ChannelModelInput::list("gpt-4o-mini"),
        ..ChannelInput::default()
    })
    .await;
}

fn bearer(key: &str) -> Vec<(&str, String)> {
    vec![("authorization", format!("Bearer {key}"))]
}

#[tokio::test]
async fn two_keys_of_one_account_share_concurrency() {
    let h = Harness::build(limits(|config| {
        config.relay_user_concurrency = 1;
    }))
    .await;
    ready(&h, Some(Duration::from_millis(400))).await;
    let second = token_svc::create_token(
        &h.pool,
        &h.config.encryption_key,
        h.user_id,
        "other",
        -1,
        true,
        None,
        "",
        None,
    )
    .await
    .unwrap();
    let body = chat_body();
    let first_headers = bearer(&h.key);
    let second_headers = bearer(&second.raw_key);
    let (a, b) = tokio::join!(
        h.call("/v1/chat/completions", &first_headers, body.clone()),
        h.call("/v1/chat/completions", &second_headers, body),
    );
    let mut statuses = [a.0.as_u16(), b.0.as_u16()];
    statuses.sort();
    assert_eq!(statuses, [200, 429]);
    assert_eq!(h.upstream_requests().await.len(), 1);
    assert_eq!(h.state.user_in_flight(h.user_id), 0);
}

#[tokio::test]
async fn instance_cap_rejects_before_upstream() {
    let h = Harness::build(limits(|config| {
        config.relay_instance_concurrency = 1;
        config.relay_user_concurrency = 8;
    }))
    .await;
    ready(&h, Some(Duration::from_millis(400))).await;
    let other = user_svc::create_user(&h.pool, "u2", "secret2", "user", 10_000_000, "default")
        .await
        .unwrap();
    let other_key = token_svc::create_token(
        &h.pool,
        &h.config.encryption_key,
        other.id,
        "tok",
        -1,
        true,
        None,
        "",
        None,
    )
    .await
    .unwrap();
    let body = chat_body();
    let a_headers = bearer(&h.key);
    let b_headers = bearer(&other_key.raw_key);
    let (a, b) = tokio::join!(
        h.call("/v1/chat/completions", &a_headers, body.clone()),
        h.call("/v1/chat/completions", &b_headers, body),
    );
    let mut statuses = [a.0.as_u16(), b.0.as_u16()];
    statuses.sort();
    assert_eq!(statuses, [200, 429]);
    assert_eq!(h.upstream_requests().await.len(), 1);
}

#[tokio::test]
async fn user_qps_rejects_the_next_call() {
    let h = Harness::build(limits(|config| {
        config.relay_user_qps = 1;
    }))
    .await;
    ready(&h, None).await;
    let body = chat_body();
    let (status, _, _) = h
        .call("/v1/chat/completions", &h.bearer(), body.clone())
        .await;
    assert_eq!(status, 200);
    let (status, headers, _) = h.call("/v1/chat/completions", &h.bearer(), body).await;
    assert_eq!(status, 429);
    assert_eq!(headers["retry-after"], "1");
    assert_eq!(h.upstream_requests().await.len(), 1);
    assert_eq!(h.state.user_in_flight(h.user_id), 0);
}

#[tokio::test]
async fn forged_key_spends_nothing_and_skips_the_database() {
    let h = Harness::new().await;
    ready(&h, None).await;
    h.pool.close().await;
    let forged = "sk-v1.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.AAAAAAAAAAAAAAAAAAAAAA".to_string();
    let (status, _, bytes) = h
        .call("/v1/chat/completions", &bearer(&forged), chat_body())
        .await;
    assert_eq!(status, 401, "{}", String::from_utf8_lossy(&bytes));
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["type"], "unauthorized");
    assert!(h.upstream_requests().await.is_empty());
    assert_eq!(h.state.instance_in_flight(), 0);
    assert_eq!(h.state.legacy_in_flight(), 0);
    assert_eq!(h.state.user_in_flight(h.user_id), 0);
}

#[tokio::test]
async fn legacy_keys_cannot_fill_the_verified_budget() {
    let h = Harness::build(limits(|config| {
        config.relay_legacy_concurrency = 0;
        config.relay_instance_concurrency = 1;
    }))
    .await;
    ready(&h, None).await;
    let legacy = format!("sk-{}", "ab".repeat(24));
    let (status, _, _) = h
        .call("/v1/chat/completions", &bearer(&legacy), chat_body())
        .await;
    assert_eq!(status, 429);
    assert!(h.upstream_requests().await.is_empty());
    assert_eq!(h.state.instance_in_flight(), 0);
    let (status, _, _) = h
        .call("/v1/chat/completions", &h.bearer(), chat_body())
        .await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn valid_legacy_key_uses_the_account_limit() {
    let h = Harness::build(limits(|config| {
        config.relay_user_concurrency = 1;
    }))
    .await;
    ready(&h, Some(Duration::from_millis(400))).await;
    let raw = format!("sk-{}", "cd".repeat(24));
    let hash = crypto::hash_token_key(&raw);
    sqlx::query(
        "INSERT INTO tokens (user_id, name, key_hash, key_prefix, status, remain_quota, unlimited_quota, models) VALUES (?, 'legacy', ?, ?, 'active', -1, 1, '')",
    )
    .bind(h.user_id)
    .bind(&hash)
    .bind(&raw[..10])
    .execute(&h.pool)
    .await
    .unwrap();
    let body = chat_body();
    let headers = bearer(&raw);
    let (a, b) = tokio::join!(
        h.call("/v1/chat/completions", &headers, body.clone()),
        h.call("/v1/chat/completions", &headers, body),
    );
    let mut statuses = [a.0.as_u16(), b.0.as_u16()];
    statuses.sort();
    assert_eq!(statuses, [200, 429]);
    assert_eq!(h.upstream_requests().await.len(), 1);
    assert_eq!(h.state.user_in_flight(h.user_id), 0);
    assert_eq!(h.state.legacy_in_flight(), 0);
}

#[tokio::test]
async fn dropping_a_stream_releases_the_account_slot() {
    let h = Harness::build(limits(|config| {
        config.relay_user_concurrency = 1;
    }))
    .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n",
            "text/event-stream",
        ))
        .mount(&h.mock)
        .await;
    h.channel(ChannelInput {
        models: danmo_core::services::channel::ChannelModelInput::list("gpt-4o-mini"),
        ..ChannelInput::default()
    })
    .await;

    let app = danmo_relay::router(h.state.clone());
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", h.key))
        .body(axum::body::Body::from(
            r#"{"model":"gpt-4o-mini","stream":true,"messages":[{"role":"user","content":"hi"}]}"#,
        ))
        .unwrap();
    let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(h.state.user_in_flight(h.user_id), 1);
    drop(resp);
    assert_eq!(h.state.user_in_flight(h.user_id), 0);
}
