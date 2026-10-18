use danmo_core::config::Config;
use danmo_core::db::connect;
use danmo_core::services::{channel, token as token_svc, user as user_svc};
use danmo_relay::RelayState;
use std::sync::Arc;
use tempfile::tempdir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn relays_chat_completions_to_mock_upstream() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("relay.db");
    let mut config = Config::from_env().unwrap();
    config.database_url = format!("sqlite://{}?mode=rwc", db_path.display());
    config.relay_max_retries = 2;
    let config = Arc::new(config);
    let pool = connect(&config).await.unwrap();

    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "hello" },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 2,
                "total_tokens": 7
            }
        })))
        .mount(&mock)
        .await;

    let user = user_svc::create_user(&pool, "u1", "secret1", "user", 100_000, "default")
        .await
        .unwrap();
    let created = token_svc::create_token(&pool, user.id, "tok", -1, true, None, "", None)
        .await
        .unwrap();

    channel::create_channel(
        &pool,
        &config.encryption_key,
        channel::ChannelInput {
            name: "mock".into(),
            type_: "openai_compat".into(),
            key: "sk-upstream".into(),
            base_url: mock.uri(),
            models: "gpt-4o-mini".into(),
            group_name: "default".into(),
            priority: 1,
            weight: 1,
            status: "enabled".into(),
        },
    )
    .await
    .unwrap();

    let state = RelayState::new(pool.clone(), config.clone());
    let app = danmo_relay::router(state);

    let body = serde_json::json!({
        "model": "gpt-4o-mini",
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": 16
    });

    let response = {
        use tower::ServiceExt;
        app.oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", created.raw_key))
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
    };

    assert_eq!(response.status(), 200);
    use http_body_util::BodyExt;
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["choices"][0]["message"]["content"], "hello");

    let logs = danmo_core::services::log::list_logs(&pool, Some(user.id), 10, 0)
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].status, "success");
    assert_eq!(logs[0].prompt_tokens, 5);
    assert_eq!(logs[0].completion_tokens, 2);
}
