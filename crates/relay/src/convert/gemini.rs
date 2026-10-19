//! Gemini generateContent <-> IR.

use super::ir::*;
use crate::sse;
use bytes::Bytes;
use danmo_core::usage::{Usage, UsageAdapter};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Request fields the Gemini codec reads.
pub const REQUEST_KEYS: &[&str] = &[
    "contents",
    "systemInstruction",
    "generationConfig",
    "tools",
    "toolConfig",
];

const RESPONSE_KEYS: &[&str] = &["candidates", "usageMetadata", "modelVersion", "responseId"];

pub fn decode_request(body: &Value) -> Result<Request, String> {
    let gen = body.get("generationConfig").cloned().unwrap_or_default();
    let mut req = Request {
        max_tokens: gen.get("maxOutputTokens").and_then(Value::as_i64),
        temperature: gen.get("temperature").and_then(Value::as_f64),
        top_p: gen.get("topP").and_then(Value::as_f64),
        stop: gen
            .get("stopSequences")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        ..Request::default()
    };
    if let Some(tc) = gen.get("thinkingConfig") {
        let budget = tc.get("thinkingBudget").and_then(Value::as_i64);
        let level = str_field(tc, "thinkingLevel").map(|l| l.to_lowercase());
        if budget == Some(0) {
            req.reasoning = Some(Reasoning::off());
        } else if budget.is_some() || level.is_some() {
            req.reasoning = Some(Reasoning {
                effort: level,
                budget_tokens: budget.filter(|b| *b > 0),
                ..Reasoning::default()
            });
        }
    }
    if let Some(sys) = body.get("systemInstruction") {
        let t = sys.get("parts").map(text_of).unwrap_or_default();
        if !t.is_empty() {
            req.system.push(t);
        }
    }

    let contents = body
        .get("contents")
        .and_then(Value::as_array)
        .ok_or("contents is required")?;
    // Gemini call ids are optional; pair responses with the oldest open call of that name.
    let mut open_calls: Vec<(String, String)> = Vec::new();
    let mut counter = 0usize;
    for c in contents {
        let role = match c.get("role").and_then(Value::as_str) {
            Some("model") => Role::Assistant,
            _ => Role::User,
        };
        let mut parts = Vec::new();
        for p in c
            .get("parts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(fc) = p.get("functionCall") {
                let name = str_field(fc, "name").unwrap_or_default();
                let id = str_field(fc, "id").unwrap_or_else(|| {
                    counter += 1;
                    format!("call_{counter}")
                });
                open_calls.push((id.clone(), name.clone()));
                parts.push(Part::ToolCall {
                    id,
                    name,
                    arguments: fc.get("args").cloned().unwrap_or(json!({})).to_string(),
                });
            } else if let Some(fr) = p.get("functionResponse") {
                let name = str_field(fr, "name").unwrap_or_default();
                let id = match str_field(fr, "id") {
                    Some(id) => {
                        open_calls.retain(|(i, _)| *i != id);
                        id
                    }
                    None => match open_calls.iter().position(|(_, n)| *n == name) {
                        Some(pos) => open_calls.remove(pos).0,
                        None => format!("call_{name}"),
                    },
                };
                let response = fr.get("response").cloned().unwrap_or_default();
                let content = match response.get("result").or_else(|| response.get("output")) {
                    Some(Value::String(s)) => s.clone(),
                    _ => response.to_string(),
                };
                parts.push(Part::ToolResult {
                    id,
                    name: Some(name),
                    content,
                    is_error: response.get("error").is_some(),
                });
            } else if let Some(d) = p.get("inlineData") {
                parts.push(Part::Image {
                    url: data_url(
                        d.get("mimeType")
                            .and_then(Value::as_str)
                            .unwrap_or("image/png"),
                        d.get("data").and_then(Value::as_str).unwrap_or_default(),
                    ),
                });
            } else if let Some(f) = p.get("fileData") {
                if let Some(uri) = str_field(f, "fileUri") {
                    parts.push(Part::Image { url: uri });
                }
            } else if let Some(t) = p.get("text").and_then(Value::as_str) {
                if p.get("thought").and_then(Value::as_bool) == Some(true) {
                    parts.push(Part::Reasoning {
                        text: t.to_string(),
                        signature: None,
                    });
                } else {
                    parts.push(Part::Text(t.to_string()));
                }
            }
        }
        push_parts(&mut req.messages, role, parts);
    }

    for t in body
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for f in t
            .get("functionDeclarations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            req.tools.push(Tool {
                name: str_field(f, "name").unwrap_or_default(),
                description: str_field(f, "description"),
                parameters: f
                    .get("parametersJsonSchema")
                    .or_else(|| f.get("parameters"))
                    .cloned()
                    .unwrap_or(Value::Null),
            });
        }
    }
    if let Some(cfg) = body.pointer("/toolConfig/functionCallingConfig") {
        let allowed = cfg
            .get("allowedFunctionNames")
            .and_then(Value::as_array)
            .filter(|a| a.len() == 1)
            .and_then(|a| a[0].as_str());
        req.tool_choice = match (cfg.get("mode").and_then(Value::as_str), allowed) {
            (Some("ANY"), Some(name)) => Some(ToolChoice::Named(name.to_string())),
            (Some("ANY"), None) => Some(ToolChoice::Required),
            (Some("NONE"), _) => Some(ToolChoice::None),
            (Some(_), _) => Some(ToolChoice::Auto),
            _ => None,
        };
    }
    Ok(req)
}

fn image_part(url: &str) -> Value {
    match split_data_url(url) {
        Some((mime, data)) => json!({"inlineData": {"mimeType": mime, "data": data}}),
        None => {
            let mime = if url.ends_with(".png") {
                "image/png"
            } else {
                "image/jpeg"
            };
            json!({"fileData": {"mimeType": mime, "fileUri": url}})
        }
    }
}

fn function_response(name: &str, id: &str, content: &str, is_error: bool) -> Value {
    let response = match serde_json::from_str::<Value>(content) {
        Ok(v @ Value::Object(_)) => v,
        _ if is_error => json!({"error": content}),
        _ => json!({"result": content}),
    };
    json!({"functionResponse": {"id": id, "name": name, "response": response}})
}

pub fn encode_request(req: &Request) -> Value {
    let mut names: HashMap<String, String> = HashMap::new();
    let mut contents = Vec::new();
    for m in &req.messages {
        let mut parts = Vec::new();
        match m.role {
            Role::User => {
                // functionResponse parts answer the preceding model turn and lead this turn.
                let mut rest = Vec::new();
                for p in &m.parts {
                    match p {
                        Part::ToolResult {
                            id,
                            name,
                            content,
                            is_error,
                        } => {
                            let name = name
                                .clone()
                                .or_else(|| names.get(id).cloned())
                                .unwrap_or_default();
                            parts.push(function_response(&name, id, content, *is_error));
                        }
                        Part::Text(t) => rest.push(json!({"text": t})),
                        Part::Image { url } => rest.push(image_part(url)),
                        _ => {}
                    }
                }
                parts.extend(rest);
            }
            Role::Assistant => {
                for p in &m.parts {
                    match p {
                        Part::Reasoning { text, .. } if !text.is_empty() => {
                            parts.push(json!({"text": text, "thought": true}))
                        }
                        Part::Text(t) if !t.is_empty() => parts.push(json!({"text": t})),
                        Part::ToolCall {
                            id,
                            name,
                            arguments,
                        } => {
                            names.insert(id.clone(), name.clone());
                            parts.push(json!({"functionCall": {
                                "id": id, "name": name, "args": args_value(arguments)
                            }}));
                        }
                        _ => {}
                    }
                }
            }
        }
        if parts.is_empty() {
            continue;
        }
        let role = if m.role == Role::User {
            "user"
        } else {
            "model"
        };
        contents.push(json!({"role": role, "parts": parts}));
    }

    let mut out = Map::new();
    if !req.system.is_empty() {
        out.insert(
            "systemInstruction".into(),
            json!({"parts": [{"text": req.system.join("\n\n")}]}),
        );
    }
    out.insert("contents".into(), Value::Array(contents));
    let mut gen = Map::new();
    if let Some(n) = req.max_tokens {
        gen.insert("maxOutputTokens".into(), json!(n));
    }
    if let Some(t) = req.temperature {
        gen.insert("temperature".into(), json!(t));
    }
    if let Some(p) = req.top_p {
        gen.insert("topP".into(), json!(p));
    }
    if !req.stop.is_empty() {
        gen.insert("stopSequences".into(), json!(req.stop));
    }
    match &req.reasoning {
        Some(r) if r.is_off() => {
            gen.insert("thinkingConfig".into(), json!({"thinkingBudget": 0}));
        }
        Some(r) if r.is_on() || r.has_level() => {
            gen.insert(
                "thinkingConfig".into(),
                json!({"thinkingBudget": r.budget(), "includeThoughts": true}),
            );
        }
        _ => {}
    }
    if !gen.is_empty() {
        out.insert("generationConfig".into(), Value::Object(gen));
    }
    if !req.tools.is_empty() {
        let decls: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                let mut f = Map::new();
                f.insert("name".into(), json!(t.name));
                if let Some(d) = &t.description {
                    f.insert("description".into(), json!(d));
                }
                // parametersJsonSchema accepts full JSON Schema; `parameters` is an OpenAPI subset.
                f.insert("parametersJsonSchema".into(), clean_schema(&t.parameters));
                Value::Object(f)
            })
            .collect();
        out.insert("tools".into(), json!([{"functionDeclarations": decls}]));
    }
    if let Some(c) = &req.tool_choice {
        let cfg = match c {
            ToolChoice::Auto => json!({"mode": "AUTO"}),
            ToolChoice::None => json!({"mode": "NONE"}),
            ToolChoice::Required => json!({"mode": "ANY"}),
            ToolChoice::Named(n) => json!({"mode": "ANY", "allowedFunctionNames": [n]}),
        };
        out.insert("toolConfig".into(), json!({"functionCallingConfig": cfg}));
    }
    Value::Object(out)
}

fn stop_from(s: Option<&str>, has_tools: bool) -> Option<StopReason> {
    let s = s?;
    Some(match s {
        "MAX_TOKENS" => StopReason::Length,
        "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" => {
            StopReason::ContentFilter
        }
        _ if has_tools => StopReason::ToolUse,
        _ => StopReason::Stop,
    })
}

fn stop_str(s: StopReason) -> &'static str {
    match s {
        StopReason::Length => "MAX_TOKENS",
        StopReason::ContentFilter => "SAFETY",
        _ => "STOP",
    }
}

fn candidate_parts(body: &Value, counter: &mut usize) -> Vec<Part> {
    let mut parts = Vec::new();
    for p in body
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(fc) = p.get("functionCall") {
            *counter += 1;
            parts.push(Part::ToolCall {
                id: str_field(fc, "id").unwrap_or_else(|| format!("call_{}", *counter)),
                name: str_field(fc, "name").unwrap_or_default(),
                arguments: fc.get("args").cloned().unwrap_or(json!({})).to_string(),
            });
        } else if let Some(t) = p.get("text").and_then(Value::as_str) {
            if p.get("thought").and_then(Value::as_bool) == Some(true) {
                parts.push(Part::Reasoning {
                    text: t.to_string(),
                    signature: None,
                });
            } else {
                parts.push(Part::Text(t.to_string()));
            }
        }
    }
    parts
}

pub fn decode_response(body: &Value) -> Result<Response, String> {
    if body.get("candidates").is_none() {
        return Err("response has no candidates".into());
    }
    let mut counter = 0;
    let parts = candidate_parts(body, &mut counter);
    let has_tools = parts.iter().any(|p| matches!(p, Part::ToolCall { .. }));
    let reason = body
        .pointer("/candidates/0/finishReason")
        .and_then(Value::as_str);
    Ok(Response {
        id: str_field(body, "responseId").unwrap_or_default(),
        stop: stop_from(reason, has_tools).unwrap_or(StopReason::Stop),
        parts,
        extra: unknown_fields(body, RESPONSE_KEYS),
        message_extra: Map::new(),
    })
}

fn parts_json(parts: &[Part]) -> Vec<Value> {
    parts
        .iter()
        .filter_map(|p| match p {
            Part::Text(t) => Some(json!({"text": t})),
            Part::Reasoning { text, .. } if text.is_empty() => None,
            Part::Reasoning { text, .. } => Some(json!({"text": text, "thought": true})),
            Part::ToolCall {
                id,
                name,
                arguments,
            } => Some(
                json!({"functionCall": {"id": id, "name": name, "args": args_value(arguments)}}),
            ),
            _ => None,
        })
        .collect()
}

pub fn encode_response(
    resp: &Response,
    usage: &Usage,
    render: &dyn UsageAdapter,
    model: &str,
) -> Value {
    json!({
        "candidates": [{
            "content": {"role": "model", "parts": parts_json(&resp.parts)},
            "finishReason": stop_str(resp.stop),
            "index": 0,
        }],
        "usageMetadata": render.render(usage),
        "modelVersion": model,
        "responseId": resp.id,
    })
}

#[derive(Default)]
pub struct Decoder {
    started: bool,
    tools: usize,
    counter: usize,
}

impl Decoder {
    pub fn event(&mut self, json: &Value) -> Vec<Event> {
        let mut out = Vec::new();
        if !self.started {
            self.started = true;
            out.push(Event::Start {
                id: str_field(json, "responseId").unwrap_or_default(),
                model: str_field(json, "modelVersion").unwrap_or_default(),
            });
        }
        for p in candidate_parts(json, &mut self.counter) {
            match p {
                Part::Text(t) => out.push(Event::Text(t)),
                Part::Reasoning { text, .. } => out.push(Event::Reasoning(text)),
                Part::ToolCall {
                    id,
                    name,
                    arguments,
                } => {
                    let index = self.tools;
                    self.tools += 1;
                    out.push(Event::ToolCallStart { index, id, name });
                    out.push(Event::ToolCallArgs {
                        index,
                        delta: arguments,
                    });
                }
                _ => {}
            }
        }
        let reason = json
            .pointer("/candidates/0/finishReason")
            .and_then(Value::as_str);
        if let Some(s) = stop_from(reason, self.tools > 0) {
            out.push(Event::Finish(s));
        }
        out
    }
}

pub struct Encoder {
    id: String,
    model: String,
    /// Gemini sends whole function calls; arguments are buffered until the call is complete.
    pending: Vec<(usize, String, String, String)>,
    stop: Option<StopReason>,
}

impl Encoder {
    pub fn new(model: String) -> Self {
        Self {
            id: gen_id(""),
            model,
            pending: Vec::new(),
            stop: None,
        }
    }

    fn chunk(&self, parts: Vec<Value>, extra: Option<(StopReason, Value)>) -> Bytes {
        let mut cand = json!({"content": {"role": "model", "parts": parts}, "index": 0});
        let mut body = json!({"modelVersion": self.model, "responseId": self.id});
        if let Some((stop, usage)) = extra {
            cand["finishReason"] = json!(stop_str(stop));
            body["usageMetadata"] = usage;
        }
        body["candidates"] = json!([cand]);
        sse::encode(None, &body)
    }

    fn flush_calls(&mut self) -> Vec<Value> {
        self.pending
            .drain(..)
            .map(|(_, id, name, args)| {
                json!({"functionCall": {"id": id, "name": name, "args": args_value(&args)}})
            })
            .collect()
    }

    pub fn event(&mut self, e: Event) -> Vec<Bytes> {
        let mut out = Vec::new();
        let flush_first = matches!(e, Event::Text(_) | Event::Reasoning(_));
        if flush_first && !self.pending.is_empty() {
            let calls = self.flush_calls();
            out.push(self.chunk(calls, None));
        }
        match e {
            Event::Start { id, .. } => {
                if !id.is_empty() {
                    self.id = id;
                }
            }
            Event::Text(t) => out.push(self.chunk(vec![json!({"text": t})], None)),
            Event::Reasoning(t) => {
                out.push(self.chunk(vec![json!({"text": t, "thought": true})], None))
            }
            Event::ReasoningSignature(_) => {}
            Event::ToolCallStart { index, id, name } => {
                self.pending.push((index, id, name, String::new()))
            }
            Event::ToolCallArgs { index, delta } => {
                if let Some(p) = self.pending.iter_mut().find(|p| p.0 == index) {
                    p.3.push_str(&delta);
                }
            }
            Event::Finish(s) => self.stop = Some(s),
        }
        out
    }

    pub fn finish(&mut self, usage: Option<&Usage>, render: &dyn UsageAdapter) -> Vec<Bytes> {
        let calls = self.flush_calls();
        let stop = self.stop.unwrap_or(StopReason::Stop);
        let usage = usage.map(|u| render.render(u)).unwrap_or(Value::Null);
        vec![self.chunk(calls, Some((stop, usage)))]
    }
}
