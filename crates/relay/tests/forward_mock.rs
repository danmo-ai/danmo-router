mod common;

use common::{data_lines, sse, Harness, USER_QUOTA};
use danmo_core::billing::{self, ModelPriceInput};
use danmo_core::services::channel::{ChannelInput, ChannelModelInput};
use serde_json::{json, Value};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

fn chat_channel(models: &str) -> ChannelInput {
    ChannelInput {
        name: "mock".into(),
        models: ChannelModelInput::list(models),
        ..Default::default()
    }
}

#[tokio::test]
async fn relays_chat_completions_to_mock_upstream() {
    let h = Harness::new().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer sk-upstream"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "hello" },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7 }
        })))
        .mount(&h.mock)
        .await;
    h.channel(chat_channel("gpt-4o-mini")).await;

    let body = json!({
        "model": "gpt-4o-mini",
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": 16
    });
    let (status, _, bytes) = h
        .call("/v1/chat/completions", &h.bearer(), body.to_string())
        .await;
    assert_eq!(status, 200);
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["choices"][0]["message"]["content"], "hello");

    let log = h.last_log().await;
    assert_eq!(log.status, "success");
    assert_eq!(log.prompt_tokens, 5);
    assert_eq!(log.completion_tokens, 2);
    assert_eq!(log.usage_adapter, "openai_chat");
}

#[tokio::test]
async fn passthrough_forwards_body_bytes_and_only_remaps_model() {
    let h = Harness::new().await;
    let upstream_body = r#"{"usage":{"prompt_tokens":3,"completion_tokens":1,"total_tokens":4},"id":"x","choices":[]}"#;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(upstream_body, "application/json"))
        .mount(&h.mock)
        .await;
    h.channel(ChannelInput {
        models: vec![
            ChannelModelInput::with("deepseek-v4-flash", "deepseek-ai/DeepSeek-V4-Flash", ""),
            ChannelModelInput::new("glm-5"),
        ],
        ..chat_channel("")
    })
    .await;

    // Odd spacing and vendor-specific fields must reach the upstream untouched.
    let raw = r#"{ "model":"glm-5", "thinking":{"type":"enabled"},"messages":[{"role":"user","content":"hi"}], "zz_vendor":1 }"#;
    let (status, _, bytes) = h.call("/v1/chat/completions", &h.bearer(), raw).await;
    assert_eq!(status, 200);
    assert_eq!(&bytes[..], upstream_body.as_bytes());

    let mapped = r#"{"model":"deepseek-v4-flash","reasoning_effort":"high","messages":[{"role":"user","content":"hi"}]}"#;
    h.call("/v1/chat/completions", &h.bearer(), mapped).await;

    let reqs = h.upstream_requests().await;
    assert_eq!(&reqs[0].body[..], raw.as_bytes());
    assert_eq!(
        String::from_utf8(reqs[1].body.clone()).unwrap(),
        r#"{"model":"deepseek-ai/DeepSeek-V4-Flash","reasoning_effort":"high","messages":[{"role":"user","content":"hi"}]}"#
    );
}

#[tokio::test]
async fn injected_stream_usage_is_billed_but_hidden_from_client() {
    let h = Harness::new().await;
    let upstream = sse(&[
        r#"data: {"id":"c","choices":[{"index":0,"delta":{"content":"hi"},"finish_reason":null}]}"#,
        r#"data: {"id":"c","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#,
        r#"data: {"id":"c","choices":[],"usage":{"prompt_tokens":11,"completion_tokens":2,"total_tokens":13,"prompt_cache_hit_tokens":8,"prompt_cache_miss_tokens":3}}"#,
        "data: [DONE]",
    ]);
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(upstream, "text/event-stream"))
        .mount(&h.mock)
        .await;
    h.channel(chat_channel("deepseek-chat")).await;

    let body =
        r#"{"model":"deepseek-chat","stream":true,"messages":[{"role":"user","content":"hi"}]}"#;
    let (status, headers, bytes) = h.call("/v1/chat/completions", &h.bearer(), body).await;
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "text/event-stream");
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("usage"), "{text}");
    assert!(text.contains("[DONE]"));
    assert_eq!(data_lines(&bytes).len(), 2);

    let sent: Value = serde_json::from_slice(&h.upstream_requests().await[0].body).unwrap();
    assert_eq!(sent["stream_options"]["include_usage"], true);

    let log = h.last_log().await;
    assert_eq!(log.status, "success", "{:?}", log.error);
    assert_eq!(log.usage_adapter, "deepseek_chat");
    assert_eq!(log.prompt_tokens, 11);
    assert_eq!(log.cache_read_tokens, 8);
}

#[tokio::test]
async fn requested_stream_usage_is_forwarded_untouched() {
    let h = Harness::new().await;
    let usage_event = r#"data: {"id":"c","choices":[],"usage":{"prompt_tokens":4,"completion_tokens":1,"total_tokens":5}}"#;
    let upstream = sse(&[
        r#"data: {"id":"c","choices":[{"index":0,"delta":{"content":"x"}}]}"#,
        usage_event,
        "data: [DONE]",
    ]);
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(upstream.clone(), "text/event-stream"),
        )
        .mount(&h.mock)
        .await;
    h.channel(chat_channel("qwen-plus")).await;

    let body = r#"{"model":"qwen-plus","stream":true,"stream_options":{"include_usage":true},"messages":[]}"#;
    let (_, _, bytes) = h.call("/v1/chat/completions", &h.bearer(), body).await;
    assert_eq!(String::from_utf8_lossy(&bytes), upstream);
    assert_eq!(&h.upstream_requests().await[0].body[..], body.as_bytes());
    assert_eq!(h.last_log().await.status, "success");
}

#[tokio::test]
async fn usage_is_rewritten_into_model_native_field_set() {
    let h = Harness::new().await;
    let mut price = ModelPriceInput::new("deepseek-chat", 1.0, 1.0);
    price.native_dialects = r#"{"openai_chat":"thinking"}"#.into();
    billing::upsert_model_price(&h.pool, &price).await.unwrap();

    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "x",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}}],
            "usage": {"prompt_tokens": 100, "completion_tokens": 10, "total_tokens": 110,
                      "prompt_tokens_details": {"cached_tokens": 64}}
        })))
        .mount(&h.mock)
        .await;
    h.channel(chat_channel("deepseek-chat")).await;

    let body = r#"{"model":"deepseek-chat","messages":[]}"#;
    let (_, _, bytes) = h.call("/v1/chat/completions", &h.bearer(), body).await;
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["usage"]["prompt_cache_hit_tokens"], 64);
    assert_eq!(v["usage"]["prompt_cache_miss_tokens"], 36);
    assert_eq!(v["choices"][0]["message"]["content"], "ok");
    assert_eq!(h.last_log().await.usage_adapter, "openai_chat");
}

#[tokio::test]
async fn missing_usage_fails_billing_and_refunds() {
    let h = Harness::new().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "x", "choices": [{"index": 0, "message": {"content": "free?"}}]
        })))
        .mount(&h.mock)
        .await;
    h.channel(chat_channel("m")).await;

    let (status, _, bytes) = h
        .call(
            "/v1/chat/completions",
            &h.bearer(),
            r#"{"model":"m","messages":[]}"#,
        )
        .await;
    assert_eq!(status, 502);
    assert!(String::from_utf8_lossy(&bytes).contains("billing failed"));
    let log = h.last_log().await;
    assert_eq!(log.status, "billing_failed");
    assert_eq!(h.used_quota().await, 0);
}

#[tokio::test]
async fn minimax_soft_error_is_a_billing_failure() {
    let h = Harness::new().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "base_resp": {"status_code": 1008, "status_msg": "insufficient balance"}
        })))
        .mount(&h.mock)
        .await;
    h.channel(ChannelInput {
        default_dialect: "minimax".into(),
        ..chat_channel("MiniMax-M2")
    })
    .await;
    let (status, _, bytes) = h
        .call(
            "/v1/chat/completions",
            &h.bearer(),
            r#"{"model":"MiniMax-M2","messages":[]}"#,
        )
        .await;
    assert_eq!(status, 502);
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["type"], "upstream_error");
    assert!(v["error"]["message"].as_str().unwrap().contains("1008"));
    assert_eq!(h.used_quota().await, 0);
    let log = h.last_log().await;
    assert_eq!(log.status, "billing_failed");
    assert!(log.error.unwrap().contains("1008"));
}

#[tokio::test]
async fn stream_without_usage_fails_billing_in_the_books() {
    let h = Harness::new().await;
    let upstream = sse(&[
        r#"data: {"id":"c","choices":[{"index":0,"delta":{"content":"x"}}]}"#,
        "data: [DONE]",
    ]);
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(upstream, "text/event-stream"))
        .mount(&h.mock)
        .await;
    h.channel(ChannelInput {
        inject_stream_usage: false,
        ..chat_channel("m")
    })
    .await;
    let (status, _, _) = h
        .call(
            "/v1/chat/completions",
            &h.bearer(),
            r#"{"model":"m","stream":true,"messages":[]}"#,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(h.last_log().await.status, "billing_failed");
    assert_eq!(h.used_quota().await, 0);
}

#[tokio::test]
async fn upstream_error_passes_through_and_refunds() {
    let h = Harness::new().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": {"message": "bad tool schema", "type": "invalid_request_error"}
        })))
        .mount(&h.mock)
        .await;
    h.channel(chat_channel("m")).await;
    let (status, _, bytes) = h
        .call(
            "/v1/chat/completions",
            &h.bearer(),
            r#"{"model":"m","messages":[]}"#,
        )
        .await;
    assert_eq!(status, 400);
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["message"], "bad tool schema");
    assert_eq!(h.last_log().await.status, "error");
    assert_eq!(h.used_quota().await, 0);
}

#[tokio::test]
async fn messages_passthrough_with_x_api_key() {
    let h = Harness::new().await;
    let upstream = sse(&[
        r#"event: message_start
data: {"type":"message_start","message":{"id":"m1","type":"message","role":"assistant","content":[],"model":"glm-5","usage":{"input_tokens":20,"cache_read_input_tokens":80,"output_tokens":1}}}"#,
        r#"event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#,
        r#"event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":7}}"#,
        r#"event: message_stop
data: {"type":"message_stop"}"#,
    ]);
    Mock::given(method("POST"))
        .and(path("/api/anthropic/v1/messages"))
        .and(header("x-api-key", "sk-upstream"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(upstream.clone(), "text/event-stream"),
        )
        .mount(&h.mock)
        .await;
    h.channel(ChannelInput {
        base_url: format!("{}/api/anthropic", h.mock.uri()),
        upstream_protocol: "anthropic_messages".into(),
        auth_style: "x_api_key".into(),
        ..chat_channel("glm-5")
    })
    .await;

    let body = r#"{"model":"glm-5","max_tokens":64,"stream":true,"messages":[{"role":"user","content":"hi"}]}"#;
    let headers = vec![
        ("x-api-key", h.key.clone()),
        ("anthropic-version", "2023-06-01".to_string()),
        (
            "anthropic-beta",
            "interleaved-thinking-2025-05-14".to_string(),
        ),
    ];
    let (status, _, bytes) = h.call("/v1/messages", &headers, body).await;
    assert_eq!(status, 200);
    assert_eq!(String::from_utf8_lossy(&bytes), upstream);

    let req = &h.upstream_requests().await[0];
    assert_eq!(&req.body[..], body.as_bytes());
    assert_eq!(
        req.headers.get("anthropic-beta").unwrap(),
        "interleaved-thinking-2025-05-14"
    );
    assert!(req.headers.get("authorization").is_none());

    let log = h.last_log().await;
    assert_eq!(log.status, "success", "{:?}", log.error);
    assert_eq!(log.usage_adapter, "anthropic_messages");
    assert_eq!(log.prompt_tokens, 100);
    assert_eq!(log.cache_read_tokens, 80);
    assert_eq!(log.completion_tokens, 7);
}

#[tokio::test]
async fn gemini_passthrough_with_query_key() {
    let h = Harness::new().await;
    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.5-flash:streamGenerateContent"))
        .and(query_param("alt", "sse"))
        .and(header("x-goog-api-key", "sk-upstream"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            sse(&[
                r#"data: {"candidates":[{"content":{"parts":[{"text":"hi"}],"role":"model"}}]}"#,
                r#"data: {"candidates":[{"content":{"parts":[{"text":"!"}],"role":"model"},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":9,"candidatesTokenCount":2,"thoughtsTokenCount":5,"totalTokenCount":16}}"#,
            ]),
            "text/event-stream",
        ))
        .mount(&h.mock)
        .await;
    h.channel(ChannelInput {
        upstream_protocol: "gemini".into(),
        auth_style: "x_goog_api_key".into(),
        ..chat_channel("gemini-2.5-flash")
    })
    .await;

    let uri = format!(
        "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse&key={}",
        h.key
    );
    let body = r#"{"contents":[{"role":"user","parts":[{"text":"hi"}]}]}"#;
    let (status, _, bytes) = h.call(&uri, &[], body).await;
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&bytes));
    assert_eq!(data_lines(&bytes).len(), 2);
    let log = h.last_log().await;
    assert_eq!(log.status, "success", "{:?}", log.error);
    assert_eq!(log.completion_tokens, 7);
    assert_eq!(log.reasoning_tokens, 5);
}

#[tokio::test]
async fn embeddings_passthrough() {
    let h = Harness::new().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "object": "list",
            "data": [{"object": "embedding", "index": 0, "embedding": [0.1]}],
            "usage": {"prompt_tokens": 8, "total_tokens": 8}
        })))
        .mount(&h.mock)
        .await;
    h.channel(ChannelInput {
        upstream_protocol: "embeddings".into(),
        ..chat_channel("text-embedding-3-small")
    })
    .await;
    let (status, _, _) = h
        .call(
            "/v1/embeddings",
            &h.bearer(),
            r#"{"model":"text-embedding-3-small","input":"hi"}"#,
        )
        .await;
    assert_eq!(status, 200);
    let log = h.last_log().await;
    assert_eq!(log.usage_adapter, "embeddings");
    assert_eq!(log.prompt_tokens, 8);
    assert!(h.used_quota().await > 0 && h.used_quota().await < USER_QUOTA);
}

#[tokio::test]
async fn messages_errors_use_anthropic_shape() {
    let h = Harness::new().await;
    let (status, _, bytes) = h
        .call(
            "/v1/messages",
            &[("x-api-key", "sk-wrong".to_string())],
            r#"{"model":"m","max_tokens":1,"messages":[]}"#,
        )
        .await;
    assert_eq!(status, 401);
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["type"], "error");
    assert_eq!(v["error"]["type"], "authentication_error");
}

fn ok_chat() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "id": "chatcmpl-1",
        "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
    }))
}

/// A preferred channel on `/flaky` that answers with `flaky`, and a fallback on `/v1`.
async fn flaky_and_fallback(h: &Harness, flaky: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/flaky/chat/completions"))
        .respond_with(flaky)
        .mount(&h.mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ok_chat())
        .mount(&h.mock)
        .await;
    h.channel(ChannelInput {
        name: "flaky".into(),
        base_url: format!("{}/flaky", h.mock.uri()),
        priority: 10,
        ..chat_channel("gpt-4o-mini")
    })
    .await;
    h.channel(ChannelInput {
        name: "fallback".into(),
        ..chat_channel("gpt-4o-mini")
    })
    .await;
}

async fn flaky_hits(h: &Harness) -> usize {
    h.upstream_requests()
        .await
        .iter()
        .filter(|r| r.url.path().starts_with("/flaky/"))
        .count()
}

const CHAT: &str = r#"{"model":"gpt-4o-mini","messages":[{"role":"user","content":"hi"}]}"#;

#[tokio::test]
async fn failing_channel_is_ejected_after_threshold() {
    let h = Harness::with_config(|c| c.health.failure_threshold = 2).await;
    flaky_and_fallback(&h, ResponseTemplate::new(503)).await;

    for _ in 0..5 {
        let (status, _, _) = h.call("/v1/chat/completions", &h.bearer(), CHAT).await;
        assert_eq!(status, 200);
    }
    assert_eq!(flaky_hits(&h).await, 2);
}

#[tokio::test]
async fn retry_after_ejects_rate_limited_channel() {
    let h = Harness::new().await;
    flaky_and_fallback(
        &h,
        ResponseTemplate::new(429).insert_header("retry-after", "60"),
    )
    .await;

    for _ in 0..3 {
        let (status, _, _) = h.call("/v1/chat/completions", &h.bearer(), CHAT).await;
        assert_eq!(status, 200);
    }
    assert_eq!(flaky_hits(&h).await, 1);
}

#[tokio::test]
async fn disabled_health_keeps_trying_failing_channel() {
    let h = Harness::with_config(|c| {
        c.health.enabled = false;
        c.health.failure_threshold = 1;
    })
    .await;
    flaky_and_fallback(&h, ResponseTemplate::new(503)).await;

    for _ in 0..3 {
        let (status, _, _) = h.call("/v1/chat/completions", &h.bearer(), CHAT).await;
        assert_eq!(status, 200);
    }
    assert_eq!(flaky_hits(&h).await, 3);
}

#[tokio::test]
async fn exhausted_retries_keep_the_upstream_status() {
    let h = Harness::new().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(529))
        .mount(&h.mock)
        .await;
    h.channel(chat_channel("gpt-4o-mini")).await;

    let (status, _, bytes) = h.call("/v1/chat/completions", &h.bearer(), CHAT).await;
    assert_eq!(status, 529);
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["message"], "upstream status 529");
    assert_eq!(h.used_quota().await, 0);
}
