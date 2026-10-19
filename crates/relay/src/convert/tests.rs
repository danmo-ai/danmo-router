use super::ir::{Event, Part, Role, StopReason};
use super::*;
use crate::collect;
use crate::sse::SseParser;
use danmo_core::usage::default_adapter;
use serde_json::json;

const CHAT_PROTOCOLS: [Protocol; 4] = [
    Protocol::OpenaiChat,
    Protocol::OpenaiResponses,
    Protocol::AnthropicMessages,
    Protocol::Gemini,
];

fn sample_request(p: Protocol) -> Value {
    match p {
        Protocol::OpenaiChat => json!({
            "model": "m",
            "messages": [
                {"role": "system", "content": "be brief"},
                {"role": "user", "content": [
                    {"type": "text", "text": "weather?"},
                    {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}
                ]},
                {"role": "assistant", "content": null, "tool_calls": [
                    {"id": "call_1", "type": "function",
                     "function": {"name": "get_weather", "arguments": "{\"city\":\"Paris\"}"}}
                ]},
                {"role": "tool", "tool_call_id": "call_1", "content": "sunny"},
            ],
            "tools": [{"type": "function", "function": {
                "name": "get_weather", "description": "d",
                "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}
            }}],
            "max_tokens": 100
        }),
        Protocol::OpenaiResponses => json!({
            "model": "m",
            "instructions": "be brief",
            "input": [
                {"type": "message", "role": "user", "content": [
                    {"type": "input_text", "text": "weather?"},
                    {"type": "input_image", "image_url": "data:image/png;base64,AAAA"}
                ]},
                {"type": "function_call", "call_id": "call_1", "name": "get_weather",
                 "arguments": "{\"city\":\"Paris\"}"},
                {"type": "function_call_output", "call_id": "call_1", "output": "sunny"}
            ],
            "tools": [{"type": "function", "name": "get_weather", "description": "d",
                       "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}}],
            "max_output_tokens": 100
        }),
        Protocol::AnthropicMessages => json!({
            "model": "m",
            "system": [{"type": "text", "text": "be brief"}],
            "max_tokens": 100,
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "weather?"},
                    {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"}}
                ]},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "call_1", "name": "get_weather", "input": {"city": "Paris"}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "call_1", "content": "sunny"}
                ]}
            ],
            "tools": [{"name": "get_weather", "description": "d",
                       "input_schema": {"type": "object", "properties": {"city": {"type": "string"}}}}]
        }),
        Protocol::Gemini => json!({
            "systemInstruction": {"parts": [{"text": "be brief"}]},
            "contents": [
                {"role": "user", "parts": [
                    {"text": "weather?"},
                    {"inlineData": {"mimeType": "image/png", "data": "AAAA"}}
                ]},
                {"role": "model", "parts": [
                    {"functionCall": {"id": "call_1", "name": "get_weather", "args": {"city": "Paris"}}}
                ]},
                {"role": "user", "parts": [
                    {"functionResponse": {"id": "call_1", "name": "get_weather", "response": {"result": "sunny"}}}
                ]}
            ],
            "tools": [{"functionDeclarations": [{"name": "get_weather", "description": "d",
                "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}}]}],
            "generationConfig": {"maxOutputTokens": 100}
        }),
        Protocol::Embeddings => unreachable!(),
    }
}

/// Semantics that must survive any direction.
fn summary(req: &ir::Request) -> Value {
    let mut parts = Vec::new();
    for m in &req.messages {
        for p in &m.parts {
            let role = if m.role == Role::User {
                "user"
            } else {
                "assistant"
            };
            match p {
                Part::Text(t) => parts.push(json!([role, "text", t])),
                Part::Image { url } => parts.push(json!([role, "image", url])),
                Part::ToolCall {
                    id,
                    name,
                    arguments,
                } => parts.push(json!([role, "call", id, name, ir::args_value(arguments)])),
                Part::ToolResult { id, content, .. } => {
                    parts.push(json!([role, "result", id, content]))
                }
                Part::Reasoning { .. } => {}
            }
        }
    }
    json!({
        "system": req.system.join("\n"),
        "parts": parts,
        "tools": req.tools.iter().map(|t| &t.name).collect::<Vec<_>>(),
        "max_tokens": req.max_tokens,
    })
}

#[test]
fn requests_convert_in_all_twelve_directions() {
    for from in CHAT_PROTOCOLS {
        let expected = summary(&decode_request(from, &sample_request(from)).unwrap());
        for to in CHAT_PROTOCOLS {
            if from == to {
                continue;
            }
            let encoded = request(from, to, &sample_request(from), "up-model", true).unwrap();
            let back = decode_request(to, &encoded).unwrap();
            assert_eq!(summary(&back), expected, "{from} -> {to}: {encoded}");
            match to {
                Protocol::Gemini => assert!(encoded.get("model").is_none()),
                _ => assert_eq!(encoded["model"], "up-model", "{from} -> {to}"),
            }
        }
    }
}

#[test]
fn chat_upstream_stream_requests_usage() {
    let out = request(
        Protocol::AnthropicMessages,
        Protocol::OpenaiChat,
        &sample_request(Protocol::AnthropicMessages),
        "m",
        true,
    )
    .unwrap();
    assert_eq!(out["stream"], true);
    assert_eq!(out["stream_options"]["include_usage"], true);
}

#[test]
fn tool_schema_without_required_is_never_null() {
    let body = json!({
        "model": "m", "max_tokens": 10,
        "messages": [{"role": "user", "content": "hi"}],
        "tools": [{"name": "t", "input_schema": {"type": "object", "required": null}}]
    });
    for to in [
        Protocol::OpenaiChat,
        Protocol::OpenaiResponses,
        Protocol::Gemini,
    ] {
        let out = request(Protocol::AnthropicMessages, to, &body, "m", false).unwrap();
        let text = out.to_string();
        assert!(!text.contains("\"required\":null"), "{to}: {text}");
        assert!(text.contains("\"properties\":{}"), "{to}: {text}");
    }
}

#[test]
fn chat_reasoning_content_maps_to_signed_thinking_only() {
    let body = json!({
        "model": "m",
        "messages": [
            {"role": "user", "content": "q"},
            {"role": "assistant", "reasoning_content": "hmm", "content": "a"},
            {"role": "user", "content": "q2"}
        ]
    });
    let out = request(
        Protocol::OpenaiChat,
        Protocol::AnthropicMessages,
        &body,
        "m",
        false,
    )
    .unwrap();
    // Unsigned reasoning cannot be replayed to Anthropic.
    assert_eq!(
        out["messages"][1]["content"],
        json!([{"type": "text", "text": "a"}])
    );

    let msgs = json!({
        "model": "m", "max_tokens": 10,
        "thinking": {"type": "enabled", "budget_tokens": 2048},
        "messages": [
            {"role": "user", "content": "q"},
            {"role": "assistant", "content": [
                {"type": "thinking", "thinking": "hmm", "signature": "sig"},
                {"type": "text", "text": "a"}
            ]},
            {"role": "user", "content": "q2"}
        ]
    });
    let out = request(
        Protocol::AnthropicMessages,
        Protocol::OpenaiChat,
        &msgs,
        "m",
        false,
    )
    .unwrap();
    assert_eq!(out["messages"][1]["reasoning_content"], "hmm");
    assert_eq!(out["reasoning_effort"], "low");
    let back = request(
        Protocol::OpenaiChat,
        Protocol::AnthropicMessages,
        &out,
        "m",
        false,
    )
    .unwrap();
    assert_eq!(back["thinking"]["type"], "enabled");
    assert!(
        back["max_tokens"].as_i64().unwrap() > back["thinking"]["budget_tokens"].as_i64().unwrap()
    );
}

#[test]
fn gemini_function_response_follows_its_call() {
    let body = json!({
        "model": "m",
        "messages": [
            {"role": "user", "content": "q"},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "a", "type": "function", "function": {"name": "f", "arguments": "{}"}},
                {"id": "b", "type": "function", "function": {"name": "g", "arguments": "{}"}}
            ]},
            {"role": "tool", "tool_call_id": "a", "content": "{\"ok\":1}"},
            {"role": "tool", "tool_call_id": "b", "content": "plain"},
            {"role": "user", "content": "thanks"}
        ]
    });
    let out = request(Protocol::OpenaiChat, Protocol::Gemini, &body, "m", false).unwrap();
    let contents = out["contents"].as_array().unwrap();
    assert_eq!(contents.len(), 3);
    assert_eq!(contents[1]["role"], "model");
    let answer = contents[2]["parts"].as_array().unwrap();
    assert_eq!(answer[0]["functionResponse"]["name"], "f");
    assert_eq!(answer[0]["functionResponse"]["response"], json!({"ok": 1}));
    assert_eq!(answer[1]["functionResponse"]["name"], "g");
    assert_eq!(
        answer[1]["functionResponse"]["response"],
        json!({"result": "plain"})
    );
    assert_eq!(answer[2]["text"], "thanks");
}

#[test]
fn messages_tool_results_lead_user_turn_and_ids_roundtrip() {
    let body = json!({
        "model": "m",
        "messages": [
            {"role": "user", "content": "q"},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "call_x", "type": "function", "function": {"name": "f", "arguments": "{\"a\":1}"}}
            ]},
            {"role": "tool", "tool_call_id": "call_x", "content": "r"},
            {"role": "user", "content": "more"}
        ]
    });
    let msgs = request(
        Protocol::OpenaiChat,
        Protocol::AnthropicMessages,
        &body,
        "m",
        false,
    )
    .unwrap();
    let turn = msgs["messages"][2]["content"].as_array().unwrap();
    assert_eq!(turn[0]["type"], "tool_result");
    assert_eq!(turn[0]["tool_use_id"], "call_x");
    assert_eq!(turn[1]["text"], "more");
    assert_eq!(msgs["messages"][1]["content"][0]["id"], "call_x");

    let chat = request(
        Protocol::AnthropicMessages,
        Protocol::OpenaiChat,
        &msgs,
        "m",
        false,
    )
    .unwrap();
    assert_eq!(chat["messages"][1]["tool_calls"][0]["id"], "call_x");
    assert_eq!(chat["messages"][2]["tool_call_id"], "call_x");
}

fn sample_usage() -> Usage {
    Usage {
        input_tokens: 100,
        cache_read_tokens: 40,
        output_tokens: 20,
        ..Usage::default()
    }
}

fn sample_response(p: Protocol) -> Value {
    match p {
        Protocol::OpenaiChat => json!({
            "id": "c1", "model": "m",
            "choices": [{"index": 0, "finish_reason": "tool_calls", "message": {
                "role": "assistant", "content": "Hello",
                "tool_calls": [{"id": "call_1", "type": "function",
                    "function": {"name": "get_weather", "arguments": "{\"city\":\"Paris\"}"}}]
            }}],
            "usage": {"prompt_tokens": 100, "completion_tokens": 20, "prompt_tokens_details": {"cached_tokens": 40}}
        }),
        Protocol::OpenaiResponses => json!({
            "id": "r1", "status": "completed", "model": "m",
            "output": [
                {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "Hello"}]},
                {"type": "function_call", "call_id": "call_1", "name": "get_weather", "arguments": "{\"city\":\"Paris\"}"}
            ],
            "usage": {"input_tokens": 100, "input_tokens_details": {"cached_tokens": 40}, "output_tokens": 20}
        }),
        Protocol::AnthropicMessages => json!({
            "id": "m1", "type": "message", "role": "assistant", "model": "m",
            "content": [
                {"type": "text", "text": "Hello"},
                {"type": "tool_use", "id": "call_1", "name": "get_weather", "input": {"city": "Paris"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 60, "cache_read_input_tokens": 40, "output_tokens": 20}
        }),
        Protocol::Gemini => json!({
            "candidates": [{"content": {"role": "model", "parts": [
                {"text": "Hello"},
                {"functionCall": {"id": "call_1", "name": "get_weather", "args": {"city": "Paris"}}}
            ]}, "finishReason": "STOP"}],
            "usageMetadata": {"promptTokenCount": 100, "cachedContentTokenCount": 40, "candidatesTokenCount": 20}
        }),
        Protocol::Embeddings => unreachable!(),
    }
}

fn decode_any_response(p: Protocol, body: &Value) -> ir::Response {
    match p {
        Protocol::OpenaiChat => chat::decode_response(body),
        Protocol::OpenaiResponses => responses::decode_response(body),
        Protocol::AnthropicMessages => messages::decode_response(body),
        Protocol::Gemini => gemini::decode_response(body),
        Protocol::Embeddings => unreachable!(),
    }
    .unwrap()
}

#[test]
fn responses_convert_in_all_twelve_directions_with_usage() {
    for from in CHAT_PROTOCOLS {
        for to in CHAT_PROTOCOLS {
            if from == to {
                continue;
            }
            let out = response(
                from,
                to,
                &sample_response(from),
                &sample_usage(),
                default_adapter(to),
                "client-model",
            )
            .unwrap();
            let back = decode_any_response(to, &out);
            assert_eq!(back.stop, StopReason::ToolUse, "{from} -> {to}");
            assert!(
                back.parts.contains(&Part::Text("Hello".into())),
                "{from} -> {to}"
            );
            let call = back
                .parts
                .iter()
                .find_map(|p| match p {
                    Part::ToolCall {
                        id,
                        name,
                        arguments,
                    } => Some((id.clone(), name.clone(), ir::args_value(arguments))),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                call,
                (
                    "call_1".into(),
                    "get_weather".into(),
                    json!({"city": "Paris"})
                )
            );

            let raw = collect::from_body(to, &out).unwrap();
            let usage = match_usage(&raw, to).unwrap();
            assert_eq!(usage.input_tokens, 100, "{from} -> {to}: {raw}");
            assert_eq!(usage.cache_read_tokens, 40, "{from} -> {to}");
            assert_eq!(usage.output_tokens, 20, "{from} -> {to}");
        }
    }
}

fn sample_stream(p: Protocol) -> String {
    let events: Vec<String> = match p {
        Protocol::OpenaiChat => vec![
            r#"data: {"id":"c","model":"m","choices":[{"index":0,"delta":{"role":"assistant","reasoning_content":"think"}}]}"#,
            r#"data: {"id":"c","choices":[{"index":0,"delta":{"content":"Hel"}}]}"#,
            r#"data: {"id":"c","choices":[{"index":0,"delta":{"content":"lo"}}]}"#,
            r#"data: {"id":"c","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"get_weather","arguments":""}}]}}]}"#,
            r#"data: {"id":"c","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"city\":"}}]}}]}"#,
            r#"data: {"id":"c","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"Paris\"}"}}]}}]}"#,
            r#"data: {"id":"c","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#,
            r#"data: {"id":"c","choices":[],"usage":{"prompt_tokens":100,"completion_tokens":20,"prompt_tokens_details":{"cached_tokens":40}}}"#,
            "data: [DONE]",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        Protocol::OpenaiResponses => vec![
            r#"event: response.created
data: {"type":"response.created","response":{"id":"r1","model":"m"}}"#,
            r#"event: response.reasoning_summary_text.delta
data: {"type":"response.reasoning_summary_text.delta","delta":"think"}"#,
            r#"event: response.output_text.delta
data: {"type":"response.output_text.delta","delta":"Hel"}"#,
            r#"event: response.output_text.delta
data: {"type":"response.output_text.delta","delta":"lo"}"#,
            r#"event: response.output_item.added
data: {"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","call_id":"call_1","name":"get_weather","arguments":""}}"#,
            r#"event: response.function_call_arguments.delta
data: {"type":"response.function_call_arguments.delta","output_index":1,"delta":"{\"city\":\"Paris\"}"}"#,
            r#"event: response.completed
data: {"type":"response.completed","response":{"id":"r1","status":"completed","usage":{"input_tokens":100,"input_tokens_details":{"cached_tokens":40},"output_tokens":20}}}"#,
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        Protocol::AnthropicMessages => vec![
            r#"event: message_start
data: {"type":"message_start","message":{"id":"m1","model":"m","usage":{"input_tokens":60,"cache_read_input_tokens":40,"output_tokens":1}}}"#,
            r#"event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"think"}}"#,
            r#"event: content_block_stop
data: {"type":"content_block_stop","index":0}"#,
            r#"event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Hello"}}"#,
            r#"event: content_block_start
data: {"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"call_1","name":"get_weather","input":{}}}"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"city\":\"Paris\"}"}}"#,
            r#"event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":20}}"#,
            r#"event: message_stop
data: {"type":"message_stop"}"#,
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        Protocol::Gemini => vec![
            r#"data: {"responseId":"g1","modelVersion":"m","candidates":[{"content":{"role":"model","parts":[{"text":"think","thought":true}]}}]}"#,
            r#"data: {"candidates":[{"content":{"role":"model","parts":[{"text":"Hello"}]}}]}"#,
            r#"data: {"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"id":"call_1","name":"get_weather","args":{"city":"Paris"}}}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":100,"cachedContentTokenCount":40,"candidatesTokenCount":20}}"#,
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        Protocol::Embeddings => unreachable!(),
    };
    events.iter().map(|e| format!("{e}\n\n")).collect()
}

/// Run `stream` (in protocol `from`) through a converter to `to`, then decode the output.
fn convert_stream(from: Protocol, to: Protocol) -> (Vec<Event>, Option<Usage>, String) {
    let mut conv = StreamConverter::new(from, to, default_adapter(to), "client".into(), true);
    let mut parser = SseParser::default();
    let mut out = Vec::new();
    // Split into small chunks to exercise buffering.
    for chunk in sample_stream(from).as_bytes().chunks(7) {
        for ev in parser.push(chunk) {
            let json = ev.json();
            out.extend(conv.event(&ev, json.as_ref()));
        }
    }
    out.extend(conv.finish());
    let text: String = out
        .iter()
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .collect();

    let mut decoder = Decoder::new(to);
    let mut collector = UsageCollector::new(to);
    let mut events = Vec::new();
    let mut parser = SseParser::default();
    for ev in parser.push(text.as_bytes()) {
        let Some(json) = ev.json() else { continue };
        let kind = ev.kind(Some(&json)).map(str::to_string);
        collector.observe(kind.as_deref(), &json);
        events.extend(decoder.event(kind.as_deref(), &json));
    }
    let usage = collector
        .finish()
        .and_then(|raw| match_usage(&raw, to).ok());
    (events, usage, text)
}

#[test]
fn streams_convert_in_all_twelve_directions_with_usage() {
    for from in CHAT_PROTOCOLS {
        for to in CHAT_PROTOCOLS {
            if from == to {
                continue;
            }
            let (events, usage, text) = convert_stream(from, to);
            let label = format!("{from} -> {to}\n{text}");
            let joined_text: String = events
                .iter()
                .filter_map(|e| match e {
                    Event::Text(t) => Some(t.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(joined_text, "Hello", "{label}");
            let reasoning: String = events
                .iter()
                .filter_map(|e| match e {
                    Event::Reasoning(t) => Some(t.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(reasoning, "think", "{label}");
            assert!(
                events.contains(&Event::ToolCallStart {
                    index: 0,
                    id: "call_1".into(),
                    name: "get_weather".into()
                }),
                "{label}"
            );
            let args: String = events
                .iter()
                .filter_map(|e| match e {
                    Event::ToolCallArgs { delta, .. } => Some(delta.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(ir::args_value(&args), json!({"city": "Paris"}), "{label}");
            assert_eq!(
                events.last(),
                Some(&Event::Finish(StopReason::ToolUse)),
                "{label}"
            );
            let usage = usage.unwrap_or_else(|| panic!("no usage: {label}"));
            assert_eq!(usage.input_tokens, 100, "{label}");
            assert_eq!(usage.cache_read_tokens, 40, "{label}");
            assert_eq!(usage.output_tokens, 20, "{label}");
        }
    }
}

#[test]
fn chat_encoder_omits_usage_chunk_unless_requested() {
    let mut conv = StreamConverter::new(
        Protocol::AnthropicMessages,
        Protocol::OpenaiChat,
        default_adapter(Protocol::OpenaiChat),
        "m".into(),
        false,
    );
    let mut parser = SseParser::default();
    let mut out = Vec::new();
    for ev in parser.push(sample_stream(Protocol::AnthropicMessages).as_bytes()) {
        let json = ev.json();
        out.extend(conv.event(&ev, json.as_ref()));
    }
    out.extend(conv.finish());
    let text: String = out
        .iter()
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .collect();
    assert!(!text.contains("\"usage\""), "{text}");
    assert!(text.ends_with("data: [DONE]\n\n"));
}
