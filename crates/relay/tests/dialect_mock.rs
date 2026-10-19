//! End to end through the relay: which dialect the upstream receives, when bytes pass through,
//! and how usage is billed and rendered.

mod common;

use common::{data_lines, sse, Harness};
use danmo_core::billing::{self, ModelPriceInput};
use danmo_core::services::channel::{ChannelInput, ChannelModelInput};
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

async fn native(h: &Harness, model: &str, dialects: Value) {
    let mut price = ModelPriceInput::new(model, 1.0, 1.0);
    price.native_dialects = dialects.to_string();
    billing::upsert_model_price(&h.pool, &price).await.unwrap();
}

fn chat_reply(usage: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "id": "x", "object": "chat.completion",
        "choices": [{"index": 0, "finish_reason": "stop",
            "message": {"role": "assistant", "content": "ok"}}],
        "usage": usage
    }))
}

fn sent(req: &wiremock::Request) -> Value {
    serde_json::from_slice(&req.body).unwrap()
}

#[tokio::test]
async fn qianfan_rows_get_their_own_dialects() {
    let h = Harness::new().await;
    native(&h, "deepseek-v4-flash", json!({"openai_chat": "thinking"})).await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(chat_reply(json!({"prompt_tokens": 100, "completion_tokens": 10,
            "total_tokens": 110, "prompt_tokens_details": {"cached_tokens": 40, "search_tokens": 0}})))
        .mount(&h.mock)
        .await;
    h.channel(ChannelInput {
        name: "qianfan".into(),
        vendor: "qianfan".into(),
        default_dialect: "qianfan".into(),
        models: vec![
            ChannelModelInput::new("deepseek-v4-flash"),
            ChannelModelInput::with("ernie-5.0", "", "qianfan_ernie"),
        ],
        ..Default::default()
    })
    .await;

    let ds = r#"{"model":"deepseek-v4-flash","thinking":{"type":"enabled"},"reasoning_effort":"high","zz_client":7,
        "messages":[{"role":"user","content":"hi"},{"role":"assistant","content":"a","reasoning_content":""},{"role":"user","content":"go"}]}"#;
    let (status, _, bytes) = h.call("/v1/chat/completions", &h.bearer(), ds).await;
    assert_eq!(status, 200);
    let client: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(client["usage"]["prompt_cache_hit_tokens"], 40);
    assert_eq!(client["usage"]["prompt_cache_miss_tokens"], 60);
    let log = h.last_log().await;
    assert_eq!(log.usage_adapter, "qianfan_chat");
    assert_eq!(log.cache_read_tokens, 40);

    let ernie = r#"{"model":"ernie-5.0","reasoning_effort":"low","messages":[{"role":"user","content":"hi"}]}"#;
    let (status, _, _) = h.call("/v1/chat/completions", &h.bearer(), ernie).await;
    assert_eq!(status, 200);

    let reqs = h.upstream_requests().await;
    let up = sent(&reqs[0]);
    assert_eq!(up["thinking"], json!({"type": "enabled"}));
    assert_eq!(up["thinking_budget"], 24576);
    assert!(up.get("reasoning_effort").is_none());
    assert_eq!(
        up["zz_client"], 7,
        "unknown client fields reach the upstream"
    );
    assert_eq!(up["messages"][1]["reasoning_content"], "");

    let up = sent(&reqs[1]);
    assert_eq!(up["enable_thinking"], true);
    assert_eq!(up["thinking_budget"], 2048);
    assert!(up.get("thinking").is_none());
    assert!(up.get("reasoning_effort").is_none());
}

#[tokio::test]
async fn same_model_on_another_vendor_uses_that_vendors_spec() {
    let h = Harness::new().await;
    native(&h, "deepseek-v4-flash", json!({"openai_chat": "thinking"})).await;
    Mock::given(method("POST"))
        .respond_with(chat_reply(
            json!({"prompt_tokens": 3, "completion_tokens": 1, "total_tokens": 4}),
        ))
        .mount(&h.mock)
        .await;
    h.channel(ChannelInput {
        name: "bailian".into(),
        default_dialect: "enable_thinking".into(),
        models: ChannelModelInput::list("deepseek-v4-flash"),
        ..Default::default()
    })
    .await;

    let body = r#"{"model":"deepseek-v4-flash","thinking":{"type":"disabled"},"messages":[{"role":"user","content":"hi"}]}"#;
    let (status, _, _) = h.call("/v1/chat/completions", &h.bearer(), body).await;
    assert_eq!(status, 200);
    let up = sent(&h.upstream_requests().await[0]);
    assert_eq!(up["enable_thinking"], false);
    assert!(up.get("thinking").is_none());
}

#[tokio::test]
async fn matching_dialects_pass_bytes_through_on_each_protocol() {
    let h = Harness::new().await;
    native(
        &h,
        "deepseek-v4-flash",
        json!({"openai_chat": "thinking", "anthropic_messages": "standard"}),
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(chat_reply(
            json!({"prompt_tokens": 9, "completion_tokens": 1,
            "total_tokens": 10, "prompt_cache_hit_tokens": 8, "prompt_cache_miss_tokens": 1}),
        ))
        .mount(&h.mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "m", "type": "message", "role": "assistant", "model": "x",
            "content": [{"type": "text", "text": "ok"}], "stop_reason": "end_turn",
            "usage": {"input_tokens": 4, "output_tokens": 2}
        })))
        .mount(&h.mock)
        .await;
    for (name, protocol, auth, dialect) in [
        ("ds-chat", "openai_chat", "bearer", "thinking"),
        ("ds-messages", "anthropic_messages", "x_api_key", "standard"),
    ] {
        h.channel(ChannelInput {
            name: name.into(),
            upstream_protocol: protocol.into(),
            auth_style: auth.into(),
            default_dialect: dialect.into(),
            models: ChannelModelInput::list("deepseek-v4-flash"),
            ..Default::default()
        })
        .await;
    }

    let chat = r#"{"model":"deepseek-v4-flash", "thinking":{"type":"enabled"}, "messages":[{"role":"assistant","content":"","reasoning_content":""},{"role":"user","content":"hi"}]}"#;
    let (status, _, _) = h.call("/v1/chat/completions", &h.bearer(), chat).await;
    assert_eq!(status, 200);
    let msgs = r#"{"model":"deepseek-v4-flash", "max_tokens":8, "messages":[{"role":"user","content":"hi"}], "zz":1}"#;
    let (status, _, _) = h.call("/v1/messages", &h.bearer(), msgs).await;
    assert_eq!(status, 200);

    let reqs = h.upstream_requests().await;
    assert_eq!(reqs[0].url.path(), "/v1/chat/completions");
    assert_eq!(&reqs[0].body[..], chat.as_bytes());
    assert_eq!(reqs[1].url.path(), "/v1/messages");
    assert_eq!(&reqs[1].body[..], msgs.as_bytes());
}

#[tokio::test]
async fn opaque_rows_pass_through_and_bill_by_field_features() {
    let h = Harness::new().await;
    native(&h, "deepseek-v4-flash", json!({"openai_chat": "thinking"})).await;
    let upstream = r#"{"id":"x","choices":[],"usage":{"prompt_tokens":100,"completion_tokens":10,"total_tokens":110,"prompt_cache_hit_tokens":64,"prompt_cache_miss_tokens":36}}"#;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(upstream, "application/json"))
        .mount(&h.mock)
        .await;
    h.channel(ChannelInput {
        name: "openrouter".into(),
        relay_mode: "convert".into(),
        default_dialect: "opaque".into(),
        models: ChannelModelInput::list("deepseek-v4-flash"),
        ..Default::default()
    })
    .await;

    let body = r#"{"model":"deepseek-v4-flash","provider":{"order":["deepseek"]},"messages":[]}"#;
    let (status, _, bytes) = h.call("/v1/chat/completions", &h.bearer(), body).await;
    assert_eq!(status, 200);
    assert_eq!(&bytes[..], upstream.as_bytes());
    assert_eq!(&h.upstream_requests().await[0].body[..], body.as_bytes());
    let log = h.last_log().await;
    assert_eq!(log.usage_adapter, "deepseek_chat");
    assert_eq!(log.cache_read_tokens, 64);

    let (status, _, _) = h
        .call(
            "/v1/messages",
            &h.bearer(),
            r#"{"model":"deepseek-v4-flash","max_tokens":8,"messages":[{"role":"user","content":"hi"}]}"#,
        )
        .await;
    assert_ne!(status, 200);
    assert_eq!(
        h.upstream_requests().await.len(),
        1,
        "opaque rows never convert"
    );
}

#[tokio::test]
async fn minimax_stream_reasoning_reaches_standard_clients() {
    let h = Harness::new().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            sse(&[
                r#"data: {"id":"c","choices":[{"index":0,"delta":{"reasoning_details":[{"type":"reasoning.text","text":"hm"}]}}]}"#,
                r#"data: {"id":"c","choices":[{"index":0,"delta":{"content":"ok"},"finish_reason":"stop"}]}"#,
                r#"data: {"id":"c","choices":[],"usage":{"prompt_tokens":5,"completion_tokens":2,"total_tokens":7},"base_resp":{"status_code":0}}"#,
                "data: [DONE]",
            ]),
            "text/event-stream",
        ))
        .mount(&h.mock)
        .await;
    h.channel(ChannelInput {
        name: "minimax".into(),
        default_dialect: "minimax".into(),
        models: ChannelModelInput::list("MiniMax-M2"),
        ..Default::default()
    })
    .await;

    let body = r#"{"model":"MiniMax-M2","stream":true,"stream_options":{"include_usage":true},"reasoning_effort":"high","messages":[{"role":"user","content":"hi"}]}"#;
    let (status, _, bytes) = h.call("/v1/chat/completions", &h.bearer(), body).await;
    assert_eq!(status, 200);
    let chunks = data_lines(&bytes);
    assert!(chunks
        .iter()
        .any(|c| c["choices"][0]["delta"]["reasoning_content"] == "hm"));
    assert_eq!(chunks.last().unwrap()["usage"]["prompt_tokens"], 5);
    let up = sent(&h.upstream_requests().await[0]);
    assert_eq!(up["thinking"], json!({"type": "adaptive"}));
    assert_eq!(up["reasoning_split"], true);
    assert_eq!(h.last_log().await.status, "success");
}
