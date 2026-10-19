//! OpenAI Chat Completions <-> IR.

use super::ir::*;
use crate::sse;
use bytes::Bytes;
use danmo_core::usage::Usage;
use danmo_core::usage::UsageAdapter;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Request fields the Chat codec reads; anything else is a dialect field or an extra.
pub const REQUEST_KEYS: &[&str] = &[
    "model",
    "messages",
    "max_tokens",
    "max_completion_tokens",
    "temperature",
    "top_p",
    "stop",
    "stream",
    "stream_options",
    "reasoning_effort",
    "tools",
    "tool_choice",
];

const MESSAGE_KEYS: &[&str] = &[
    "role",
    "content",
    "reasoning_content",
    "reasoning",
    "reasoning_details",
    "tool_calls",
];

const RESPONSE_KEYS: &[&str] = &["id", "object", "created", "model", "choices", "usage"];

/// Reasoning text of a message or delta. An empty `reasoning_content` is kept: thinking-mode
/// upstreams require it echoed back on assistant turns.
fn reasoning_of(m: &Value) -> Option<String> {
    if let Some(r) = m
        .get("reasoning_content")
        .or_else(|| m.get("reasoning"))
        .and_then(Value::as_str)
    {
        return Some(r.to_string());
    }
    let details = m.get("reasoning_details")?.as_array()?;
    Some(
        details
            .iter()
            .filter_map(|d| d.get("text").and_then(Value::as_str))
            .collect(),
    )
}

pub fn decode_request(body: &Value) -> Result<Request, String> {
    let mut req = Request {
        max_tokens: body
            .get("max_completion_tokens")
            .or_else(|| body.get("max_tokens"))
            .and_then(Value::as_i64),
        temperature: body.get("temperature").and_then(Value::as_f64),
        top_p: body.get("top_p").and_then(Value::as_f64),
        stream: body.get("stream").and_then(Value::as_bool).unwrap_or(false),
        ..Request::default()
    };
    match body.get("stop") {
        Some(Value::String(s)) => req.stop.push(s.clone()),
        Some(Value::Array(a)) => req
            .stop
            .extend(a.iter().filter_map(|s| s.as_str().map(str::to_string))),
        _ => {}
    }
    match body.get("reasoning_effort").and_then(Value::as_str) {
        Some("none") => req.reasoning = Some(Reasoning::off()),
        Some(effort) => {
            req.reasoning = Some(Reasoning {
                effort: Some(effort.to_string()),
                ..Reasoning::default()
            })
        }
        None => {}
    }

    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or("messages is required")?;
    for m in messages {
        let role = m.get("role").and_then(Value::as_str).unwrap_or("user");
        match role {
            "system" | "developer" => {
                let text = text_of(m.get("content").unwrap_or(&Value::Null));
                if !text.is_empty() {
                    req.system.push(text);
                }
            }
            "assistant" => {
                let mut parts = Vec::new();
                if let Some(r) = reasoning_of(m) {
                    parts.push(Part::Reasoning {
                        text: r,
                        signature: None,
                    });
                }
                let text = text_of(m.get("content").unwrap_or(&Value::Null));
                if !text.is_empty() {
                    parts.push(Part::Text(text));
                }
                for tc in m
                    .get("tool_calls")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    parts.push(Part::ToolCall {
                        id: str_field(tc, "id").unwrap_or_else(|| gen_id("call_")),
                        name: tc
                            .pointer("/function/name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        arguments: tc
                            .pointer("/function/arguments")
                            .and_then(Value::as_str)
                            .unwrap_or("{}")
                            .to_string(),
                    });
                }
                push_parts_with(
                    &mut req.messages,
                    Role::Assistant,
                    parts,
                    unknown_fields(m, MESSAGE_KEYS),
                );
            }
            "tool" => {
                let part = Part::ToolResult {
                    id: str_field(m, "tool_call_id").unwrap_or_default(),
                    name: str_field(m, "name"),
                    content: text_of(m.get("content").unwrap_or(&Value::Null)),
                    is_error: false,
                };
                push_parts(&mut req.messages, Role::User, vec![part]);
            }
            _ => {
                let parts = user_parts(m.get("content").unwrap_or(&Value::Null));
                push_parts_with(
                    &mut req.messages,
                    Role::User,
                    parts,
                    unknown_fields(m, MESSAGE_KEYS),
                );
            }
        }
    }

    for t in body
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(f) = t.get("function") else { continue };
        req.tools.push(Tool {
            name: str_field(f, "name").unwrap_or_default(),
            description: str_field(f, "description"),
            parameters: f.get("parameters").cloned().unwrap_or(Value::Null),
        });
    }
    req.tool_choice = match body.get("tool_choice") {
        Some(Value::String(s)) => match s.as_str() {
            "none" => Some(ToolChoice::None),
            "required" => Some(ToolChoice::Required),
            _ => Some(ToolChoice::Auto),
        },
        Some(v) => v
            .pointer("/function/name")
            .and_then(Value::as_str)
            .map(|n| ToolChoice::Named(n.to_string())),
        None => None,
    };
    Ok(req)
}

fn user_parts(content: &Value) -> Vec<Part> {
    match content {
        Value::Array(items) => items
            .iter()
            .filter_map(|p| match p.get("type").and_then(Value::as_str) {
                Some("image_url") => p
                    .pointer("/image_url/url")
                    .or_else(|| p.get("image_url"))
                    .and_then(Value::as_str)
                    .map(|u| Part::Image { url: u.to_string() }),
                _ => p
                    .get("text")
                    .and_then(Value::as_str)
                    .map(|t| Part::Text(t.to_string())),
            })
            .collect(),
        other => {
            let t = text_of(other);
            if t.is_empty() {
                Vec::new()
            } else {
                vec![Part::Text(t)]
            }
        }
    }
}

pub fn encode_request(req: &Request, model: &str) -> Value {
    let mut messages = Vec::new();
    if !req.system.is_empty() {
        messages.push(json!({"role": "system", "content": req.system.join("\n\n")}));
    }
    for m in &req.messages {
        match m.role {
            Role::User => {
                let mut content = Vec::new();
                for p in &m.parts {
                    match p {
                        Part::ToolResult { id, content: c, .. } => {
                            messages
                                .push(json!({"role": "tool", "tool_call_id": id, "content": c}));
                        }
                        Part::Text(t) => content.push(json!({"type": "text", "text": t})),
                        Part::Image { url } => {
                            content.push(json!({"type": "image_url", "image_url": {"url": url}}))
                        }
                        _ => {}
                    }
                }
                if content.is_empty() {
                    continue;
                }
                let only_text = content.iter().all(|c| c["type"] == "text");
                let value = if only_text {
                    json!(content
                        .iter()
                        .map(|c| c["text"].as_str().unwrap_or_default())
                        .collect::<Vec<_>>()
                        .join(""))
                } else {
                    Value::Array(content)
                };
                let mut msg = Map::new();
                msg.insert("role".into(), json!("user"));
                msg.insert("content".into(), value);
                insert_missing(&mut msg, &m.extra);
                messages.push(Value::Object(msg));
            }
            Role::Assistant => {
                let mut text = String::new();
                let mut reasoning: Option<String> = None;
                let mut calls = Vec::new();
                for p in &m.parts {
                    match p {
                        Part::Text(t) => text.push_str(t),
                        Part::Reasoning { text: r, .. } => {
                            reasoning.get_or_insert_with(String::new).push_str(r)
                        }
                        Part::ToolCall {
                            id,
                            name,
                            arguments,
                        } => calls.push(json!({
                            "id": id,
                            "type": "function",
                            "function": {"name": name, "arguments": arguments}
                        })),
                        _ => {}
                    }
                }
                let mut msg = Map::new();
                msg.insert("role".into(), json!("assistant"));
                msg.insert(
                    "content".into(),
                    if text.is_empty() && !calls.is_empty() {
                        Value::Null
                    } else {
                        json!(text)
                    },
                );
                if let Some(r) = reasoning {
                    msg.insert("reasoning_content".into(), json!(r));
                }
                if !calls.is_empty() {
                    msg.insert("tool_calls".into(), Value::Array(calls));
                }
                insert_missing(&mut msg, &m.extra);
                messages.push(Value::Object(msg));
            }
        }
    }

    let mut out = Map::new();
    out.insert("model".into(), json!(model));
    out.insert("messages".into(), Value::Array(messages));
    if let Some(n) = req.max_tokens {
        out.insert("max_tokens".into(), json!(n));
    }
    if let Some(t) = req.temperature {
        out.insert("temperature".into(), json!(t));
    }
    if let Some(p) = req.top_p {
        out.insert("top_p".into(), json!(p));
    }
    if !req.stop.is_empty() {
        out.insert("stop".into(), json!(req.stop));
    }
    match &req.reasoning {
        Some(r) if r.is_off() => {
            out.insert("reasoning_effort".into(), json!("none"));
        }
        Some(r) if r.has_level() => {
            out.insert("reasoning_effort".into(), json!(r.effort()));
        }
        _ => {}
    }
    if !req.tools.is_empty() {
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                let mut f = Map::new();
                f.insert("name".into(), json!(t.name));
                if let Some(d) = &t.description {
                    f.insert("description".into(), json!(d));
                }
                f.insert("parameters".into(), clean_schema(&t.parameters));
                json!({"type": "function", "function": f})
            })
            .collect();
        out.insert("tools".into(), Value::Array(tools));
    }
    if let Some(choice) = &req.tool_choice {
        let v = match choice {
            ToolChoice::Auto => json!("auto"),
            ToolChoice::None => json!("none"),
            ToolChoice::Required => json!("required"),
            ToolChoice::Named(n) => json!({"type": "function", "function": {"name": n}}),
        };
        out.insert("tool_choice".into(), v);
    }
    if req.stream {
        out.insert("stream".into(), json!(true));
        out.insert("stream_options".into(), json!({"include_usage": true}));
    }
    Value::Object(out)
}

fn stop_from(s: Option<&str>) -> StopReason {
    match s {
        Some("length") => StopReason::Length,
        Some("tool_calls") | Some("function_call") => StopReason::ToolUse,
        Some("content_filter") => StopReason::ContentFilter,
        _ => StopReason::Stop,
    }
}

fn stop_str(s: StopReason) -> &'static str {
    match s {
        StopReason::Stop => "stop",
        StopReason::Length => "length",
        StopReason::ToolUse => "tool_calls",
        StopReason::ContentFilter => "content_filter",
    }
}

pub fn decode_response(body: &Value) -> Result<Response, String> {
    let choice = body
        .pointer("/choices/0")
        .ok_or("response has no choices")?;
    let msg = choice.get("message").cloned().unwrap_or(json!({}));
    let mut parts = Vec::new();
    if let Some(r) = reasoning_of(&msg).filter(|s| !s.is_empty()) {
        parts.push(Part::Reasoning {
            text: r,
            signature: None,
        });
    }
    let text = text_of(msg.get("content").unwrap_or(&Value::Null));
    if !text.is_empty() {
        parts.push(Part::Text(text));
    }
    for tc in msg
        .get("tool_calls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        parts.push(Part::ToolCall {
            id: str_field(tc, "id").unwrap_or_else(|| gen_id("call_")),
            name: tc
                .pointer("/function/name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            arguments: tc
                .pointer("/function/arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}")
                .to_string(),
        });
    }
    Ok(Response {
        id: str_field(body, "id").unwrap_or_else(|| gen_id("chatcmpl-")),
        parts,
        stop: stop_from(choice.get("finish_reason").and_then(Value::as_str)),
        extra: unknown_fields(body, RESPONSE_KEYS),
        message_extra: unknown_fields(&msg, MESSAGE_KEYS),
    })
}

pub fn encode_response(
    resp: &Response,
    usage: &Usage,
    render: &dyn UsageAdapter,
    model: &str,
) -> Value {
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut calls = Vec::new();
    for p in &resp.parts {
        match p {
            Part::Text(t) => text.push_str(t),
            Part::Reasoning { text: r, .. } => reasoning.push_str(r),
            Part::ToolCall {
                id,
                name,
                arguments,
            } => calls.push(json!({
                "id": id, "type": "function",
                "function": {"name": name, "arguments": arguments}
            })),
            _ => {}
        }
    }
    let mut msg = Map::new();
    msg.insert("role".into(), json!("assistant"));
    msg.insert(
        "content".into(),
        if text.is_empty() && !calls.is_empty() {
            Value::Null
        } else {
            json!(text)
        },
    );
    if !reasoning.is_empty() {
        msg.insert("reasoning_content".into(), json!(reasoning));
    }
    if !calls.is_empty() {
        msg.insert("tool_calls".into(), Value::Array(calls));
    }
    insert_missing(&mut msg, &resp.message_extra);
    json!({
        "id": resp.id,
        "object": "chat.completion",
        "created": now_secs(),
        "model": model,
        "choices": [{"index": 0, "message": msg, "finish_reason": stop_str(resp.stop)}],
        "usage": render.render(usage),
    })
}

#[derive(Default)]
pub struct Decoder {
    started: bool,
    /// upstream tool_calls[].index -> IR tool index
    tools: HashMap<i64, usize>,
}

impl Decoder {
    pub fn event(&mut self, json: &Value) -> Vec<Event> {
        let mut out = Vec::new();
        if !self.started && json.get("choices").is_some() {
            self.started = true;
            out.push(Event::Start {
                id: str_field(json, "id").unwrap_or_default(),
                model: str_field(json, "model").unwrap_or_default(),
            });
        }
        let Some(choice) = json.pointer("/choices/0") else {
            return out;
        };
        if let Some(delta) = choice.get("delta") {
            if let Some(r) = reasoning_of(delta).filter(|s| !s.is_empty()) {
                out.push(Event::Reasoning(r));
            }
            if let Some(t) = delta
                .get("content")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                out.push(Event::Text(t.to_string()));
            }
            for tc in delta
                .get("tool_calls")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let upstream_index = tc.get("index").and_then(Value::as_i64).unwrap_or(0);
                let index = match self.tools.get(&upstream_index) {
                    Some(i) => *i,
                    None => {
                        let i = self.tools.len();
                        self.tools.insert(upstream_index, i);
                        out.push(Event::ToolCallStart {
                            index: i,
                            id: str_field(tc, "id").unwrap_or_else(|| gen_id("call_")),
                            name: tc
                                .pointer("/function/name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                        });
                        i
                    }
                };
                if let Some(a) = tc
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    out.push(Event::ToolCallArgs {
                        index,
                        delta: a.to_string(),
                    });
                }
            }
        }
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            out.push(Event::Finish(stop_from(Some(reason))));
        }
        out
    }
}

pub struct Encoder {
    id: String,
    model: String,
    created: i64,
    started: bool,
    stop: Option<StopReason>,
    include_usage: bool,
}

impl Encoder {
    pub fn new(model: String, include_usage: bool) -> Self {
        Self {
            id: gen_id("chatcmpl-"),
            model,
            created: now_secs(),
            started: false,
            stop: None,
            include_usage,
        }
    }

    fn chunk(&self, delta: Value, finish: Option<&str>) -> Bytes {
        sse::encode(
            None,
            &json!({
                "id": self.id,
                "object": "chat.completion.chunk",
                "created": self.created,
                "model": self.model,
                "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
            }),
        )
    }

    fn ensure_started(&mut self, out: &mut Vec<Bytes>) {
        if !self.started {
            self.started = true;
            out.push(self.chunk(json!({"role": "assistant", "content": ""}), None));
        }
    }

    pub fn event(&mut self, ev: Event) -> Vec<Bytes> {
        let mut out = Vec::new();
        if let Event::Start { id, .. } = &ev {
            if !id.is_empty() && !self.started {
                self.id = id.clone();
            }
        }
        self.ensure_started(&mut out);
        match ev {
            Event::Start { .. } | Event::ReasoningSignature(_) => {}
            Event::Text(t) => out.push(self.chunk(json!({"content": t}), None)),
            Event::Reasoning(r) => out.push(self.chunk(json!({"reasoning_content": r}), None)),
            Event::ToolCallStart { index, id, name } => out.push(self.chunk(
                json!({"tool_calls": [{"index": index, "id": id, "type": "function",
                    "function": {"name": name, "arguments": ""}}]}),
                None,
            )),
            Event::ToolCallArgs { index, delta } => out.push(self.chunk(
                json!({"tool_calls": [{"index": index, "function": {"arguments": delta}}]}),
                None,
            )),
            Event::Finish(s) => self.stop = Some(s),
        }
        out
    }

    pub fn finish(&mut self, usage: Option<&Usage>, render: &dyn UsageAdapter) -> Vec<Bytes> {
        let mut out = Vec::new();
        self.ensure_started(&mut out);
        let stop = stop_str(self.stop.unwrap_or(StopReason::Stop));
        out.push(self.chunk(json!({}), Some(stop)));
        if let (true, Some(u)) = (self.include_usage, usage) {
            out.push(sse::encode(
                None,
                &json!({
                    "id": self.id,
                    "object": "chat.completion.chunk",
                    "created": self.created,
                    "model": self.model,
                    "choices": [],
                    "usage": render.render(u),
                }),
            ));
        }
        out.push(Bytes::from_static(b"data: [DONE]\n\n"));
        out
    }
}
