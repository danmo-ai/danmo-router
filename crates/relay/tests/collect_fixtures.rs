//! Streaming usage shapes per protocol and vendor, driven end to end through passthrough
//! channels. Each case asserts the matched adapter and the billed numbers.

mod common;

use common::{sse, Harness};
use danmo_core::services::channel::ChannelInput;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

struct Case {
    uri: &'static str,
    protocol: &'static str,
    upstream_path: &'static str,
    body: &'static str,
    events: Vec<&'static str>,
    adapter: &'static str,
    input: i64,
    cache_read: i64,
    output: i64,
    reasoning: i64,
}

async fn run(case: Case) {
    let h = Harness::new().await;
    Mock::given(method("POST"))
        .and(path(case.upstream_path))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(sse(&case.events), "text/event-stream"),
        )
        .mount(&h.mock)
        .await;
    h.channel(ChannelInput {
        name: "c".into(),
        models: "m".into(),
        upstream_protocol: case.protocol.into(),
        ..Default::default()
    })
    .await;
    let (status, _, _) = h.call(case.uri, &h.bearer(), case.body).await;
    assert_eq!(status, 200);
    let log = h.last_log().await;
    assert_eq!(log.status, "success", "{:?} {:?}", log.error, log.raw_usage);
    assert_eq!(log.usage_adapter, case.adapter);
    assert_eq!(log.prompt_tokens, case.input);
    assert_eq!(log.cache_read_tokens, case.cache_read);
    assert_eq!(log.completion_tokens, case.output);
    assert_eq!(log.reasoning_tokens, case.reasoning);
}

const CHAT_STREAM: &str =
    r#"{"model":"m","stream":true,"messages":[{"role":"user","content":"hi"}]}"#;

#[tokio::test]
async fn openai_responses_stream() {
    run(Case {
        uri: "/v1/responses",
        protocol: "openai_responses",
        upstream_path: "/v1/responses",
        body: r#"{"model":"m","stream":true,"input":"hi"}"#,
        events: vec![
            r#"event: response.created
data: {"type":"response.created","response":{"id":"r1","status":"in_progress","usage":null}}"#,
            r#"event: response.output_text.delta
data: {"type":"response.output_text.delta","item_id":"i","output_index":0,"content_index":0,"delta":"hi"}"#,
            r#"event: response.completed
data: {"type":"response.completed","response":{"id":"r1","status":"completed","usage":{"input_tokens":30,"input_tokens_details":{"cached_tokens":10},"output_tokens":50,"output_tokens_details":{"reasoning_tokens":40},"total_tokens":80}}}"#,
        ],
        adapter: "openai_responses",
        input: 30,
        cache_read: 10,
        output: 50,
        reasoning: 40,
    })
    .await;
}

#[tokio::test]
async fn kimi_stream_usage_on_last_choice_chunk() {
    run(Case {
        uri: "/v1/chat/completions",
        protocol: "openai_chat",
        upstream_path: "/v1/chat/completions",
        body: CHAT_STREAM,
        events: vec![
            r#"data: {"id":"c","choices":[{"index":0,"delta":{"content":"hi"}}]}"#,
            r#"data: {"id":"c","choices":[{"index":0,"delta":{},"finish_reason":"stop","usage":{"prompt_tokens":19,"completion_tokens":13,"total_tokens":32,"cached_tokens":16}}],"usage":{"prompt_tokens":19,"completion_tokens":13,"total_tokens":32,"cached_tokens":16}}"#,
            "data: [DONE]",
        ],
        adapter: "kimi_chat",
        input: 19,
        cache_read: 16,
        output: 13,
        reasoning: 0,
    })
    .await;
}

#[tokio::test]
async fn doubao_ark_stream_usage_chunk() {
    run(Case {
        uri: "/v1/chat/completions",
        protocol: "openai_chat",
        upstream_path: "/v1/chat/completions",
        body: CHAT_STREAM,
        events: vec![
            r#"data: {"choices":[{"delta":{"reasoning_content":"think","role":"assistant"},"index":0}],"created":1,"id":"a","model":"doubao-seed-1-6","object":"chat.completion.chunk","usage":null}"#,
            r#"data: {"choices":[{"delta":{"content":"hi","role":"assistant"},"finish_reason":"stop","index":0}],"id":"a","object":"chat.completion.chunk","usage":null}"#,
            r#"data: {"choices":[],"id":"a","object":"chat.completion.chunk","usage":{"completion_tokens":120,"prompt_tokens":20,"total_tokens":140,"prompt_tokens_details":{"cached_tokens":0},"completion_tokens_details":{"reasoning_tokens":100}}}"#,
            "data: [DONE]",
        ],
        adapter: "openai_chat",
        input: 20,
        cache_read: 0,
        output: 120,
        reasoning: 100,
    })
    .await;
}

#[tokio::test]
async fn qianfan_stream_with_search() {
    run(Case {
        uri: "/v1/chat/completions",
        protocol: "openai_chat",
        upstream_path: "/v1/chat/completions",
        body: CHAT_STREAM,
        events: vec![
            r#"data: {"id":"q","choices":[{"index":0,"delta":{"content":"hi"}}]}"#,
            r#"data: {"id":"q","choices":[],"usage":{"prompt_tokens":900,"completion_tokens":30,"total_tokens":930,"prompt_tokens_details":{"search_tokens":700}}}"#,
            "data: [DONE]",
        ],
        adapter: "qianfan_chat",
        input: 900,
        cache_read: 0,
        output: 30,
        reasoning: 0,
    })
    .await;
}

#[tokio::test]
async fn deepseek_anthropic_stream_with_cache() {
    run(Case {
        uri: "/v1/messages",
        protocol: "anthropic_messages",
        upstream_path: "/v1/messages",
        body: r#"{"model":"m","max_tokens":10,"stream":true,"messages":[{"role":"user","content":"hi"}]}"#,
        events: vec![
            r#"event: message_start
data: {"type":"message_start","message":{"id":"x","usage":{"input_tokens":12,"cache_read_input_tokens":500,"cache_creation_input_tokens":0,"output_tokens":0}}}"#,
            r#"event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":42}}"#,
            r#"event: message_stop
data: {"type":"message_stop"}"#,
        ],
        adapter: "anthropic_messages",
        input: 512,
        cache_read: 500,
        output: 42,
        reasoning: 0,
    })
    .await;
}
