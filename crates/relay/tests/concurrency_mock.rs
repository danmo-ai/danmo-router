mod common;

use common::Harness;
use danmo_core::services::channel::{ChannelInput, ChannelModelInput};
use serde_json::json;
use std::sync::Arc;
use wiremock::matchers::method;
use wiremock::{Mock, ResponseTemplate};

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_requests_on_one_account_all_settle() {
    let h = Arc::new(Harness::new().await);
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "x",
            "choices": [{"index": 0, "message": {"content": "ok"}}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 10, "total_tokens": 20}
        })))
        .mount(&h.mock)
        .await;
    h.channel(ChannelInput {
        name: "mock".into(),
        models: ChannelModelInput::list("m"),
        ..Default::default()
    })
    .await;

    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..40 {
        let h = h.clone();
        tasks.spawn(async move {
            let (status, _, bytes) = h
                .call(
                    "/v1/chat/completions",
                    &h.bearer(),
                    r#"{"model":"m","messages":[],"max_tokens":64}"#,
                )
                .await;
            (status, String::from_utf8_lossy(&bytes).into_owned())
        });
    }
    let mut failures = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        let (status, body) = joined.unwrap();
        if status != 200 {
            failures.push(format!("{status}: {body}"));
        }
    }
    assert!(failures.is_empty(), "failed requests: {failures:#?}");
    // Unpriced model: ratio 1, so each call settles to its 20 tokens.
    assert_eq!(h.used_quota().await, 40 * 20);
}
