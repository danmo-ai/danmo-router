mod common;

use common::{data_lines, sse, Harness};
use danmo_core::billing::{self, ModelPriceInput};
use danmo_core::services::channel::{ChannelInput, ChannelModelInput};
use serde_json::{json, Value};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

fn convert_channel(models: &str, protocol: &str) -> ChannelInput {
    ChannelInput {
        name: "convert".into(),
        models: ChannelModelInput::list(models),
        relay_mode: "convert".into(),
        upstream_protocol: protocol.into(),
        auth_style: if protocol == "anthropic_messages" {
            "x_api_key".into()
        } else {
            "bearer".into()
        },
        ..Default::default()
    }
}

const MESSAGES_BODY: &str = r#"{"model":"deepseek-chat","max_tokens":256,"system":"be brief","messages":[{"role":"user","content":"hi"}]}"#;

#[tokio::test]
async fn messages_client_on_chat_upstream() {
    let h = Harness::new().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer sk-upstream"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "c1", "model": "deepseek-chat",
            "choices": [{"index": 0, "finish_reason": "stop",
                "message": {"role": "assistant", "content": "hello", "reasoning_content": "r"}}],
            "usage": {"prompt_tokens": 50, "completion_tokens": 9, "total_tokens": 59,
                      "prompt_cache_hit_tokens": 32, "prompt_cache_miss_tokens": 18}
        })))
        .mount(&h.mock)
        .await;
    h.channel(convert_channel("deepseek-chat", "openai_chat"))
        .await;

    let (status, headers, bytes) = h
        .call(
            "/v1/messages",
            &[("x-api-key", h.key.clone())],
            MESSAGES_BODY,
        )
        .await;
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&bytes));
    assert_eq!(headers["content-type"], "application/json");
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["type"], "message");
    assert_eq!(v["model"], "deepseek-chat");
    assert_eq!(v["stop_reason"], "end_turn");
    assert_eq!(v["content"][0]["type"], "thinking");
    assert_eq!(v["content"][1], json!({"type": "text", "text": "hello"}));
    assert_eq!(v["usage"]["input_tokens"], 18);
    assert_eq!(v["usage"]["cache_read_input_tokens"], 32);

    let sent: Value = serde_json::from_slice(&h.upstream_requests().await[0].body).unwrap();
    assert_eq!(
        sent["messages"][0],
        json!({"role": "system", "content": "be brief"})
    );
    assert_eq!(sent["max_tokens"], 256);

    let log = h.last_log().await;
    assert_eq!(log.status, "success", "{:?}", log.error);
    assert_eq!(log.usage_adapter, "deepseek_chat");
    assert_eq!(log.prompt_tokens, 50);
    assert_eq!(log.cache_read_tokens, 32);
}

#[tokio::test]
async fn chat_client_on_messages_upstream_stream() {
    let h = Harness::new().await;
    Mock::given(method("POST"))
        .and(path("/anthropic/v1/messages"))
        .and(header("x-api-key", "sk-upstream"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            sse(&[
                r#"event: message_start
data: {"type":"message_start","message":{"id":"m1","model":"MiniMax-M2","usage":{"input_tokens":30,"output_tokens":1}}}"#,
                r#"event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
                r#"event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hey"}}"#,
                r#"event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":4}}"#,
                r#"event: message_stop
data: {"type":"message_stop"}"#,
            ]),
            "text/event-stream",
        ))
        .mount(&h.mock)
        .await;
    h.channel(ChannelInput {
        base_url: format!("{}/anthropic", h.mock.uri()),
        ..convert_channel("MiniMax-M2", "anthropic_messages")
    })
    .await;

    let body = r#"{"model":"MiniMax-M2","stream":true,"stream_options":{"include_usage":true},"messages":[{"role":"user","content":"hi"}]}"#;
    let (status, headers, bytes) = h.call("/v1/chat/completions", &h.bearer(), body).await;
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "text/event-stream");
    let chunks = data_lines(&bytes);
    let text: String = chunks
        .iter()
        .filter_map(|c| {
            c.pointer("/choices/0/delta/content")
                .and_then(Value::as_str)
        })
        .collect();
    assert_eq!(text, "hey");
    let usage = chunks.iter().find_map(|c| c.get("usage")).unwrap();
    assert_eq!(usage["prompt_tokens"], 30);
    assert_eq!(usage["completion_tokens"], 4);
    assert!(String::from_utf8_lossy(&bytes).ends_with("data: [DONE]\n\n"));

    let sent: Value = serde_json::from_slice(&h.upstream_requests().await[0].body).unwrap();
    assert_eq!(sent["stream"], true);
    assert!(sent["max_tokens"].as_i64().unwrap() > 0);

    let log = h.last_log().await;
    assert_eq!(log.status, "success", "{:?}", log.error);
    assert_eq!(log.usage_adapter, "anthropic_messages");
    assert_eq!(log.completion_tokens, 4);
}

#[tokio::test]
async fn converted_usage_uses_model_native_field_set() {
    let h = Harness::new().await;
    let mut price = ModelPriceInput::new("deepseek-chat", 1.0, 1.0);
    price.native_dialects = r#"{"openai_chat":"thinking"}"#.into();
    billing::upsert_model_price(&h.pool, &price).await.unwrap();

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "m", "type": "message", "role": "assistant", "model": "x",
            "content": [{"type": "text", "text": "ok"}], "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "cache_read_input_tokens": 90, "output_tokens": 5}
        })))
        .mount(&h.mock)
        .await;
    h.channel(convert_channel("deepseek-chat", "anthropic_messages"))
        .await;

    let (status, _, bytes) = h
        .call(
            "/v1/chat/completions",
            &h.bearer(),
            r#"{"model":"deepseek-chat","messages":[{"role":"user","content":"hi"}]}"#,
        )
        .await;
    assert_eq!(status, 200);
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["choices"][0]["message"]["content"], "ok");
    assert_eq!(v["usage"]["prompt_tokens"], 100);
    assert_eq!(v["usage"]["prompt_cache_hit_tokens"], 90);
    assert_eq!(v["usage"]["prompt_cache_miss_tokens"], 10);
}

#[tokio::test]
async fn responses_client_on_chat_upstream_stream() {
    let h = Harness::new().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            sse(&[
                r#"data: {"id":"c","choices":[{"index":0,"delta":{"content":"yo"}}]}"#,
                r#"data: {"id":"c","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#,
                r#"data: {"id":"c","choices":[],"usage":{"prompt_tokens":7,"completion_tokens":2,"total_tokens":9}}"#,
                "data: [DONE]",
            ]),
            "text/event-stream",
        ))
        .mount(&h.mock)
        .await;
    h.channel(convert_channel("qwen-plus", "openai_chat")).await;

    let (status, _, bytes) = h
        .call(
            "/v1/responses",
            &h.bearer(),
            r#"{"model":"qwen-plus","stream":true,"input":"hi"}"#,
        )
        .await;
    assert_eq!(status, 200);
    let events = data_lines(&bytes);
    let completed = events
        .iter()
        .find(|e| e["type"] == "response.completed")
        .unwrap();
    assert_eq!(completed["response"]["usage"]["input_tokens"], 7);
    assert_eq!(
        completed["response"]["output"][0]["content"][0]["text"],
        "yo"
    );

    let sent: Value = serde_json::from_slice(&h.upstream_requests().await[0].body).unwrap();
    assert_eq!(sent["stream_options"]["include_usage"], true);
    assert_eq!(sent["messages"][0]["content"], "hi");
    assert_eq!(h.last_log().await.status, "success");
}

#[tokio::test]
async fn gemini_client_on_chat_upstream() {
    let h = Harness::new().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "c", "choices": [{"index": 0, "finish_reason": "stop",
                "message": {"role": "assistant", "content": "bonjour"}}],
            "usage": {"prompt_tokens": 3, "completion_tokens": 1, "total_tokens": 4}
        })))
        .mount(&h.mock)
        .await;
    h.channel(convert_channel("glm-5", "openai_chat")).await;

    let (status, _, bytes) = h
        .call(
            "/v1beta/models/glm-5:generateContent",
            &[("x-goog-api-key", h.key.clone())],
            r#"{"contents":[{"role":"user","parts":[{"text":"hi"}]}]}"#,
        )
        .await;
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&bytes));
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["candidates"][0]["content"]["parts"][0]["text"], "bonjour");
    assert_eq!(v["usageMetadata"]["promptTokenCount"], 3);

    let sent: Value = serde_json::from_slice(&h.upstream_requests().await[0].body).unwrap();
    assert_eq!(sent["model"], "glm-5");
}

#[tokio::test]
async fn converted_upstream_error_uses_inbound_shape() {
    let h = Harness::new().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": {"message": "context too long", "type": "invalid_request_error"}
        })))
        .mount(&h.mock)
        .await;
    h.channel(convert_channel("deepseek-chat", "openai_chat"))
        .await;

    let (status, _, bytes) = h
        .call(
            "/v1/messages",
            &[("x-api-key", h.key.clone())],
            MESSAGES_BODY,
        )
        .await;
    assert_eq!(status, 400);
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["type"], "error");
    assert_eq!(v["error"]["type"], "invalid_request_error");
    assert_eq!(v["error"]["message"], "context too long");
    assert_eq!(h.used_quota().await, 0);
}
