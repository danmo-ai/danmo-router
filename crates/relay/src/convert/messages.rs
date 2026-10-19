//! Anthropic Messages <-> IR.

use super::ir::*;
use crate::sse;
use bytes::Bytes;
use danmo_core::usage::{Usage, UsageAdapter};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

const DEFAULT_MAX_TOKENS: i64 = 8192;

pub fn decode_request(body: &Value) -> Result<Request, String> {
    let mut req = Request {
        max_tokens: body.get("max_tokens").and_then(Value::as_i64),
        temperature: body.get("temperature").and_then(Value::as_f64),
        top_p: body.get("top_p").and_then(Value::as_f64),
        stream: body.get("stream").and_then(Value::as_bool).unwrap_or(false),
        ..Request::default()
    };
    if let Some(sys) = body.get("system") {
        let t = text_of(sys);
        if !t.is_empty() {
            req.system.push(t);
        }
    }
    req.stop = body
        .get("stop_sequences")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if let Some(t) = body.get("thinking") {
        if t.get("type").and_then(Value::as_str) == Some("enabled") {
            req.reasoning = Some(Reasoning {
                effort: None,
                budget_tokens: t.get("budget_tokens").and_then(Value::as_i64),
            });
        }
    }

    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or("messages is required")?;
    for m in messages {
        let role = match m.get("role").and_then(Value::as_str) {
            Some("assistant") => Role::Assistant,
            _ => Role::User,
        };
        let parts = match m.get("content") {
            Some(Value::Array(blocks)) => blocks.iter().filter_map(block_to_part).collect(),
            Some(other) => {
                let t = text_of(other);
                if t.is_empty() {
                    Vec::new()
                } else {
                    vec![Part::Text(t)]
                }
            }
            None => Vec::new(),
        };
        push_parts(&mut req.messages, role, parts);
    }

    for t in body
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        // Server tools (web_search_*, bash_*, ...) have no schema to carry over.
        if t.get("input_schema").is_none() {
            continue;
        }
        req.tools.push(Tool {
            name: str_field(t, "name").unwrap_or_default(),
            description: str_field(t, "description"),
            parameters: t.get("input_schema").cloned().unwrap_or(Value::Null),
        });
    }
    req.tool_choice =
        body.get("tool_choice")
            .and_then(|c| match c.get("type").and_then(Value::as_str) {
                Some("auto") => Some(ToolChoice::Auto),
                Some("any") => Some(ToolChoice::Required),
                Some("none") => Some(ToolChoice::None),
                Some("tool") => str_field(c, "name").map(ToolChoice::Named),
                _ => None,
            });
    Ok(req)
}

fn block_to_part(b: &Value) -> Option<Part> {
    match b.get("type").and_then(Value::as_str)? {
        "text" => str_field(b, "text").map(Part::Text),
        "image" => {
            let src = b.get("source")?;
            match src.get("type").and_then(Value::as_str)? {
                "base64" => Some(Part::Image {
                    url: data_url(
                        src.get("media_type")
                            .and_then(Value::as_str)
                            .unwrap_or("image/png"),
                        src.get("data").and_then(Value::as_str).unwrap_or_default(),
                    ),
                }),
                "url" => str_field(src, "url").map(|url| Part::Image { url }),
                _ => None,
            }
        }
        "thinking" => Some(Part::Reasoning {
            text: str_field(b, "thinking").unwrap_or_default(),
            signature: str_field(b, "signature").filter(|s| !s.is_empty()),
        }),
        "tool_use" => Some(Part::ToolCall {
            id: str_field(b, "id").unwrap_or_else(|| gen_id("toolu_")),
            name: str_field(b, "name").unwrap_or_default(),
            arguments: b
                .get("input")
                .map(Value::to_string)
                .unwrap_or_else(|| "{}".into()),
        }),
        "tool_result" => Some(Part::ToolResult {
            id: str_field(b, "tool_use_id").unwrap_or_default(),
            name: None,
            content: text_of(b.get("content").unwrap_or(&Value::Null)),
            is_error: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
        }),
        _ => None,
    }
}

fn image_block(url: &str) -> Value {
    match split_data_url(url) {
        Some((mime, data)) => {
            json!({"type": "image", "source": {"type": "base64", "media_type": mime, "data": data}})
        }
        None => json!({"type": "image", "source": {"type": "url", "url": url}}),
    }
}

fn assistant_blocks(parts: &[Part]) -> Vec<Value> {
    let mut blocks = Vec::new();
    for p in parts {
        match p {
            // Anthropic rejects thinking blocks it did not sign.
            Part::Reasoning {
                text,
                signature: Some(sig),
            } => blocks.push(json!({"type": "thinking", "thinking": text, "signature": sig})),
            Part::Text(t) if !t.is_empty() => blocks.push(json!({"type": "text", "text": t})),
            Part::ToolCall {
                id,
                name,
                arguments,
            } => blocks.push(json!({
                "type": "tool_use", "id": id, "name": name, "input": args_value(arguments)
            })),
            _ => {}
        }
    }
    blocks
}

pub fn encode_request(req: &Request, model: &str) -> Value {
    let mut messages = Vec::new();
    for m in &req.messages {
        let blocks = match m.role {
            Role::User => {
                // tool_result blocks must lead the user turn.
                let mut results = Vec::new();
                let mut rest = Vec::new();
                for p in &m.parts {
                    match p {
                        Part::ToolResult {
                            id,
                            content,
                            is_error,
                            ..
                        } => {
                            let mut b = json!({"type": "tool_result", "tool_use_id": id, "content": content});
                            if *is_error {
                                b["is_error"] = json!(true);
                            }
                            results.push(b);
                        }
                        Part::Text(t) if !t.is_empty() => {
                            rest.push(json!({"type": "text", "text": t}))
                        }
                        Part::Image { url } => rest.push(image_block(url)),
                        _ => {}
                    }
                }
                results.extend(rest);
                results
            }
            Role::Assistant => assistant_blocks(&m.parts),
        };
        if blocks.is_empty() {
            continue;
        }
        let role = if m.role == Role::User {
            "user"
        } else {
            "assistant"
        };
        messages.push(json!({"role": role, "content": blocks}));
    }

    let mut max_tokens = req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
    let mut out = Map::new();
    out.insert("model".into(), json!(model));
    if let Some(r) = &req.reasoning {
        let budget = r.budget().max(1024);
        if max_tokens <= budget {
            max_tokens = budget + DEFAULT_MAX_TOKENS;
        }
        out.insert(
            "thinking".into(),
            json!({"type": "enabled", "budget_tokens": budget}),
        );
    }
    out.insert("max_tokens".into(), json!(max_tokens));
    if !req.system.is_empty() {
        out.insert("system".into(), json!(req.system.join("\n\n")));
    }
    out.insert("messages".into(), Value::Array(messages));
    if let Some(t) = req.temperature {
        out.insert("temperature".into(), json!(t));
    }
    if let Some(p) = req.top_p {
        out.insert("top_p".into(), json!(p));
    }
    if !req.stop.is_empty() {
        out.insert("stop_sequences".into(), json!(req.stop));
    }
    if !req.tools.is_empty() {
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                let mut v = Map::new();
                v.insert("name".into(), json!(t.name));
                if let Some(d) = &t.description {
                    v.insert("description".into(), json!(d));
                }
                v.insert("input_schema".into(), clean_schema(&t.parameters));
                Value::Object(v)
            })
            .collect();
        out.insert("tools".into(), Value::Array(tools));
    }
    if let Some(c) = &req.tool_choice {
        let v = match c {
            ToolChoice::Auto => json!({"type": "auto"}),
            ToolChoice::None => json!({"type": "none"}),
            ToolChoice::Required => json!({"type": "any"}),
            ToolChoice::Named(n) => json!({"type": "tool", "name": n}),
        };
        out.insert("tool_choice".into(), v);
    }
    if req.stream {
        out.insert("stream".into(), json!(true));
    }
    Value::Object(out)
}

fn stop_from(s: Option<&str>) -> StopReason {
    match s {
        Some("max_tokens") => StopReason::Length,
        Some("tool_use") => StopReason::ToolUse,
        Some("refusal") => StopReason::ContentFilter,
        _ => StopReason::Stop,
    }
}

fn stop_str(s: StopReason) -> &'static str {
    match s {
        StopReason::Stop => "end_turn",
        StopReason::Length => "max_tokens",
        StopReason::ToolUse => "tool_use",
        StopReason::ContentFilter => "refusal",
    }
}

pub fn decode_response(body: &Value) -> Result<Response, String> {
    let blocks = body
        .get("content")
        .and_then(Value::as_array)
        .ok_or("response has no content")?;
    Ok(Response {
        id: str_field(body, "id").unwrap_or_else(|| gen_id("msg_")),
        parts: blocks.iter().filter_map(block_to_part).collect(),
        stop: stop_from(body.get("stop_reason").and_then(Value::as_str)),
    })
}

pub fn encode_response(
    resp: &Response,
    usage: &Usage,
    render: &dyn UsageAdapter,
    model: &str,
) -> Value {
    let content: Vec<Value> = resp
        .parts
        .iter()
        .filter_map(|p| match p {
            Part::Reasoning { text, signature } => Some(json!({
                "type": "thinking", "thinking": text,
                "signature": signature.clone().unwrap_or_default()
            })),
            Part::Text(t) => Some(json!({"type": "text", "text": t})),
            Part::ToolCall {
                id,
                name,
                arguments,
            } => Some(json!({
                "type": "tool_use", "id": id, "name": name, "input": args_value(arguments)
            })),
            _ => None,
        })
        .collect();
    json!({
        "id": resp.id,
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": stop_str(resp.stop),
        "stop_sequence": null,
        "usage": render.render(usage),
    })
}

#[derive(Default)]
pub struct Decoder {
    /// content block index -> IR tool index
    tools: HashMap<i64, usize>,
}

impl Decoder {
    pub fn event(&mut self, kind: Option<&str>, json: &Value) -> Vec<Event> {
        let mut out = Vec::new();
        match kind {
            Some("message_start") => out.push(Event::Start {
                id: json
                    .pointer("/message/id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                model: json
                    .pointer("/message/model")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
            }),
            Some("content_block_start") => {
                let block = json.get("content_block").cloned().unwrap_or_default();
                if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                    let index = self.tools.len();
                    let block_index = json.get("index").and_then(Value::as_i64).unwrap_or(0);
                    self.tools.insert(block_index, index);
                    out.push(Event::ToolCallStart {
                        index,
                        id: str_field(&block, "id").unwrap_or_else(|| gen_id("toolu_")),
                        name: str_field(&block, "name").unwrap_or_default(),
                    });
                }
            }
            Some("content_block_delta") => {
                let delta = json.get("delta").cloned().unwrap_or_default();
                match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        if let Some(t) = str_field(&delta, "text") {
                            out.push(Event::Text(t));
                        }
                    }
                    Some("thinking_delta") => {
                        if let Some(t) = str_field(&delta, "thinking") {
                            out.push(Event::Reasoning(t));
                        }
                    }
                    Some("signature_delta") => {
                        if let Some(s) = str_field(&delta, "signature") {
                            out.push(Event::ReasoningSignature(s));
                        }
                    }
                    Some("input_json_delta") => {
                        let block_index = json.get("index").and_then(Value::as_i64).unwrap_or(0);
                        if let (Some(index), Some(d)) = (
                            self.tools.get(&block_index),
                            str_field(&delta, "partial_json"),
                        ) {
                            if !d.is_empty() {
                                out.push(Event::ToolCallArgs {
                                    index: *index,
                                    delta: d,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
            Some("message_delta") => {
                if let Some(r) = json.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    out.push(Event::Finish(stop_from(Some(r))));
                }
            }
            _ => {}
        }
        out
    }
}

#[derive(PartialEq)]
enum Block {
    Text,
    Thinking,
    Tool(usize),
}

pub struct Encoder {
    id: String,
    model: String,
    started: bool,
    block: Option<Block>,
    next_index: usize,
    stop: Option<StopReason>,
}

fn ev(name: &str, data: Value) -> Bytes {
    let mut map = Map::new();
    map.insert("type".into(), json!(name));
    if let Value::Object(fields) = data {
        map.extend(fields);
    }
    sse::encode(Some(name), &Value::Object(map))
}

impl Encoder {
    pub fn new(model: String) -> Self {
        Self {
            id: gen_id("msg_"),
            model,
            started: false,
            block: None,
            next_index: 0,
            stop: None,
        }
    }

    fn ensure_started(&mut self, out: &mut Vec<Bytes>) {
        if self.started {
            return;
        }
        self.started = true;
        out.push(ev(
            "message_start",
            json!({"message": {
                "id": self.id, "type": "message", "role": "assistant", "model": self.model,
                "content": [], "stop_reason": null, "stop_sequence": null,
                "usage": {"input_tokens": 0, "output_tokens": 0}
            }}),
        ));
    }

    fn close_block(&mut self, out: &mut Vec<Bytes>) {
        if self.block.take().is_some() {
            out.push(ev(
                "content_block_stop",
                json!({"index": self.next_index - 1}),
            ));
        }
    }

    fn open_block(&mut self, block: Block, content: Value, out: &mut Vec<Bytes>) {
        self.close_block(out);
        out.push(ev(
            "content_block_start",
            json!({"index": self.next_index, "content_block": content}),
        ));
        self.next_index += 1;
        self.block = Some(block);
    }

    fn delta(&self, delta: Value) -> Bytes {
        ev(
            "content_block_delta",
            json!({"index": self.next_index - 1, "delta": delta}),
        )
    }

    pub fn event(&mut self, e: Event) -> Vec<Bytes> {
        let mut out = Vec::new();
        if let Event::Start { id, .. } = &e {
            if !id.is_empty() && !self.started {
                self.id = id.clone();
            }
        }
        self.ensure_started(&mut out);
        match e {
            Event::Start { .. } => {}
            Event::Text(t) => {
                if self.block != Some(Block::Text) {
                    self.open_block(Block::Text, json!({"type": "text", "text": ""}), &mut out);
                }
                out.push(self.delta(json!({"type": "text_delta", "text": t})));
            }
            Event::Reasoning(t) => {
                if self.block != Some(Block::Thinking) {
                    self.open_block(
                        Block::Thinking,
                        json!({"type": "thinking", "thinking": "", "signature": ""}),
                        &mut out,
                    );
                }
                out.push(self.delta(json!({"type": "thinking_delta", "thinking": t})));
            }
            Event::ReasoningSignature(s) => {
                if self.block == Some(Block::Thinking) {
                    out.push(self.delta(json!({"type": "signature_delta", "signature": s})));
                }
            }
            Event::ToolCallStart { index, id, name } => self.open_block(
                Block::Tool(index),
                json!({"type": "tool_use", "id": id, "name": name, "input": {}}),
                &mut out,
            ),
            Event::ToolCallArgs { index, delta } => {
                if self.block == Some(Block::Tool(index)) {
                    out.push(
                        self.delta(json!({"type": "input_json_delta", "partial_json": delta})),
                    );
                }
            }
            Event::Finish(s) => self.stop = Some(s),
        }
        out
    }

    pub fn finish(&mut self, usage: Option<&Usage>, render: &dyn UsageAdapter) -> Vec<Bytes> {
        let mut out = Vec::new();
        self.ensure_started(&mut out);
        self.close_block(&mut out);
        let usage = usage
            .map(|u| render.render(u))
            .unwrap_or_else(|| json!({"output_tokens": 0}));
        out.push(ev(
            "message_delta",
            json!({
                "delta": {"stop_reason": stop_str(self.stop.unwrap_or(StopReason::Stop)), "stop_sequence": null},
                "usage": usage,
            }),
        ));
        out.push(ev("message_stop", json!({})));
        out
    }
}
