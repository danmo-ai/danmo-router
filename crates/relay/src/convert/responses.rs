//! OpenAI Responses <-> IR.

use super::ir::*;
use crate::sse;
use bytes::Bytes;
use danmo_core::usage::{Usage, UsageAdapter};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Request fields the Responses codec reads.
pub const REQUEST_KEYS: &[&str] = &[
    "model",
    "input",
    "instructions",
    "max_output_tokens",
    "temperature",
    "top_p",
    "stream",
    "reasoning",
    "tools",
    "tool_choice",
    "store",
];

const RESPONSE_KEYS: &[&str] = &[
    "id",
    "object",
    "created_at",
    "model",
    "status",
    "output",
    "usage",
    "incomplete_details",
];

pub fn decode_request(body: &Value) -> Result<Request, String> {
    let mut req = Request {
        max_tokens: body.get("max_output_tokens").and_then(Value::as_i64),
        temperature: body.get("temperature").and_then(Value::as_f64),
        top_p: body.get("top_p").and_then(Value::as_f64),
        stream: body.get("stream").and_then(Value::as_bool).unwrap_or(false),
        ..Request::default()
    };
    if let Some(i) = body.get("instructions").and_then(Value::as_str) {
        if !i.is_empty() {
            req.system.push(i.to_string());
        }
    }
    match body.pointer("/reasoning/effort").and_then(Value::as_str) {
        Some("none") => req.reasoning = Some(Reasoning::off()),
        Some(effort) => {
            req.reasoning = Some(Reasoning {
                effort: Some(effort.to_string()),
                ..Reasoning::default()
            })
        }
        None => {}
    }

    match body.get("input") {
        Some(Value::String(s)) => {
            push_parts(&mut req.messages, Role::User, vec![Part::Text(s.clone())])
        }
        Some(Value::Array(items)) => {
            for item in items {
                decode_input_item(item, &mut req);
            }
        }
        _ => return Err("input is required".into()),
    }

    for t in body
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if t.get("type").and_then(Value::as_str) != Some("function") {
            continue;
        }
        req.tools.push(Tool {
            name: str_field(t, "name").unwrap_or_default(),
            description: str_field(t, "description"),
            parameters: t.get("parameters").cloned().unwrap_or(Value::Null),
        });
    }
    req.tool_choice = match body.get("tool_choice") {
        Some(Value::String(s)) => match s.as_str() {
            "none" => Some(ToolChoice::None),
            "required" => Some(ToolChoice::Required),
            _ => Some(ToolChoice::Auto),
        },
        Some(v) => str_field(v, "name").map(ToolChoice::Named),
        None => None,
    };
    Ok(req)
}

fn decode_input_item(item: &Value, req: &mut Request) {
    let kind = item
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("message");
    match kind {
        "message" => {
            let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
            let content = item.get("content").unwrap_or(&Value::Null);
            if role == "system" || role == "developer" {
                let t = text_of(content);
                if !t.is_empty() {
                    req.system.push(t);
                }
                return;
            }
            let parts: Vec<Part> = match content {
                Value::Array(ps) => ps
                    .iter()
                    .filter_map(|p| match p.get("type").and_then(Value::as_str) {
                        Some("input_image") => p
                            .get("image_url")
                            .and_then(Value::as_str)
                            .map(|u| Part::Image { url: u.to_string() }),
                        _ => str_field(p, "text").map(Part::Text),
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
            };
            let role = if role == "assistant" {
                Role::Assistant
            } else {
                Role::User
            };
            push_parts(&mut req.messages, role, parts);
        }
        "function_call" => push_parts(
            &mut req.messages,
            Role::Assistant,
            vec![Part::ToolCall {
                id: str_field(item, "call_id").unwrap_or_default(),
                name: str_field(item, "name").unwrap_or_default(),
                arguments: str_field(item, "arguments").unwrap_or_else(|| "{}".into()),
            }],
        ),
        "function_call_output" => push_parts(
            &mut req.messages,
            Role::User,
            vec![Part::ToolResult {
                id: str_field(item, "call_id").unwrap_or_default(),
                name: None,
                content: text_of(item.get("output").unwrap_or(&Value::Null)),
                is_error: false,
            }],
        ),
        "reasoning" => {
            let text = item.get("summary").map(text_of).unwrap_or_default();
            if !text.is_empty() {
                push_parts(
                    &mut req.messages,
                    Role::Assistant,
                    vec![Part::Reasoning {
                        text,
                        signature: None,
                    }],
                );
            }
        }
        _ => {}
    }
}

pub fn encode_request(req: &Request, model: &str) -> Value {
    let mut input = Vec::new();
    for m in &req.messages {
        match m.role {
            Role::User => {
                let mut content = Vec::new();
                for p in &m.parts {
                    match p {
                        Part::ToolResult { id, content: c, .. } => input.push(json!({
                            "type": "function_call_output", "call_id": id, "output": c
                        })),
                        Part::Text(t) => content.push(json!({"type": "input_text", "text": t})),
                        Part::Image { url } => {
                            content.push(json!({"type": "input_image", "image_url": url}))
                        }
                        _ => {}
                    }
                }
                if !content.is_empty() {
                    input.push(json!({"type": "message", "role": "user", "content": content}));
                }
            }
            Role::Assistant => {
                for p in &m.parts {
                    match p {
                        Part::Text(t) => input.push(json!({
                            "type": "message", "role": "assistant",
                            "content": [{"type": "output_text", "text": t}]
                        })),
                        Part::ToolCall {
                            id,
                            name,
                            arguments,
                        } => input.push(json!({
                            "type": "function_call", "call_id": id, "name": name,
                            "arguments": arguments
                        })),
                        _ => {}
                    }
                }
            }
        }
    }

    let mut out = Map::new();
    out.insert("model".into(), json!(model));
    if !req.system.is_empty() {
        out.insert("instructions".into(), json!(req.system.join("\n\n")));
    }
    out.insert("input".into(), Value::Array(input));
    if let Some(n) = req.max_tokens {
        out.insert("max_output_tokens".into(), json!(n));
    }
    if let Some(t) = req.temperature {
        out.insert("temperature".into(), json!(t));
    }
    if let Some(p) = req.top_p {
        out.insert("top_p".into(), json!(p));
    }
    match &req.reasoning {
        Some(r) if r.is_off() => {
            out.insert("reasoning".into(), json!({"effort": "none"}));
        }
        Some(r) if r.has_level() || r.is_on() => {
            out.insert(
                "reasoning".into(),
                json!({"effort": r.effort(), "summary": "auto"}),
            );
        }
        _ => {}
    }
    if !req.tools.is_empty() {
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                let mut v = Map::new();
                v.insert("type".into(), json!("function"));
                v.insert("name".into(), json!(t.name));
                if let Some(d) = &t.description {
                    v.insert("description".into(), json!(d));
                }
                v.insert("parameters".into(), clean_schema(&t.parameters));
                Value::Object(v)
            })
            .collect();
        out.insert("tools".into(), Value::Array(tools));
    }
    if let Some(c) = &req.tool_choice {
        let v = match c {
            ToolChoice::Auto => json!("auto"),
            ToolChoice::None => json!("none"),
            ToolChoice::Required => json!("required"),
            ToolChoice::Named(n) => json!({"type": "function", "name": n}),
        };
        out.insert("tool_choice".into(), v);
    }
    out.insert("store".into(), json!(false));
    if req.stream {
        out.insert("stream".into(), json!(true));
    }
    Value::Object(out)
}

pub fn decode_response(body: &Value) -> Result<Response, String> {
    let items = body
        .get("output")
        .and_then(Value::as_array)
        .ok_or("response has no output")?;
    let mut parts = Vec::new();
    for item in items {
        match item.get("type").and_then(Value::as_str) {
            Some("message") => {
                let t = text_of(item.get("content").unwrap_or(&Value::Null));
                if !t.is_empty() {
                    parts.push(Part::Text(t));
                }
            }
            Some("reasoning") => {
                let t = item
                    .get("summary")
                    .map(text_of)
                    .filter(|t| !t.is_empty())
                    .or_else(|| item.get("content").map(text_of))
                    .unwrap_or_default();
                if !t.is_empty() {
                    parts.push(Part::Reasoning {
                        text: t,
                        signature: None,
                    });
                }
            }
            Some("function_call") => parts.push(Part::ToolCall {
                id: str_field(item, "call_id").unwrap_or_else(|| gen_id("call_")),
                name: str_field(item, "name").unwrap_or_default(),
                arguments: str_field(item, "arguments").unwrap_or_else(|| "{}".into()),
            }),
            _ => {}
        }
    }
    let incomplete = body.get("status").and_then(Value::as_str) == Some("incomplete");
    let mut resp = Response {
        id: str_field(body, "id").unwrap_or_else(|| gen_id("resp_")),
        parts,
        stop: StopReason::Stop,
        extra: unknown_fields(body, RESPONSE_KEYS),
        message_extra: Map::new(),
    };
    resp.stop = if incomplete {
        match body
            .pointer("/incomplete_details/reason")
            .and_then(Value::as_str)
        {
            Some("content_filter") => StopReason::ContentFilter,
            _ => StopReason::Length,
        }
    } else if resp.has_tool_calls() {
        StopReason::ToolUse
    } else {
        StopReason::Stop
    };
    Ok(resp)
}

fn output_items(parts: &[Part]) -> Vec<Value> {
    let mut items = Vec::new();
    for p in parts {
        match p {
            Part::Reasoning { text, .. } if text.is_empty() => {}
            Part::Reasoning { text, .. } => items.push(json!({
                "type": "reasoning", "id": gen_id("rs_"),
                "summary": [{"type": "summary_text", "text": text}]
            })),
            Part::Text(t) => items.push(json!({
                "type": "message", "id": gen_id("msg_"), "status": "completed",
                "role": "assistant",
                "content": [{"type": "output_text", "text": t, "annotations": []}]
            })),
            Part::ToolCall {
                id,
                name,
                arguments,
            } => items.push(json!({
                "type": "function_call", "id": gen_id("fc_"), "call_id": id,
                "name": name, "arguments": arguments, "status": "completed"
            })),
            _ => {}
        }
    }
    items
}

fn response_object(
    id: &str,
    model: &str,
    created: i64,
    stop: Option<StopReason>,
    output: Vec<Value>,
    usage: Option<Value>,
) -> Value {
    let (status, incomplete) = match stop {
        None => ("in_progress", Value::Null),
        Some(StopReason::Length) => ("incomplete", json!({"reason": "max_output_tokens"})),
        Some(StopReason::ContentFilter) => ("incomplete", json!({"reason": "content_filter"})),
        Some(_) => ("completed", Value::Null),
    };
    json!({
        "id": id,
        "object": "response",
        "created_at": created,
        "status": status,
        "model": model,
        "output": output,
        "incomplete_details": incomplete,
        "usage": usage.unwrap_or(Value::Null),
    })
}

pub fn encode_response(
    resp: &Response,
    usage: &Usage,
    render: &dyn UsageAdapter,
    model: &str,
) -> Value {
    response_object(
        &resp.id,
        model,
        now_secs(),
        Some(resp.stop),
        output_items(&resp.parts),
        Some(render.render(usage)),
    )
}

#[derive(Default)]
pub struct Decoder {
    /// output_index -> IR tool index
    tools: HashMap<i64, usize>,
    saw_tool: bool,
}

impl Decoder {
    pub fn event(&mut self, kind: Option<&str>, json: &Value) -> Vec<Event> {
        let mut out = Vec::new();
        match kind {
            Some("response.created") => out.push(Event::Start {
                id: json
                    .pointer("/response/id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                model: json
                    .pointer("/response/model")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
            }),
            Some("response.output_text.delta") => {
                if let Some(d) = str_field(json, "delta") {
                    out.push(Event::Text(d));
                }
            }
            Some("response.reasoning_summary_text.delta")
            | Some("response.reasoning_text.delta") => {
                if let Some(d) = str_field(json, "delta") {
                    out.push(Event::Reasoning(d));
                }
            }
            Some("response.output_item.added") => {
                let item = json.get("item").cloned().unwrap_or_default();
                if item.get("type").and_then(Value::as_str) == Some("function_call") {
                    let index = self.tools.len();
                    let oi = json
                        .get("output_index")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    self.tools.insert(oi, index);
                    self.saw_tool = true;
                    out.push(Event::ToolCallStart {
                        index,
                        id: str_field(&item, "call_id").unwrap_or_else(|| gen_id("call_")),
                        name: str_field(&item, "name").unwrap_or_default(),
                    });
                    if let Some(a) = str_field(&item, "arguments").filter(|a| !a.is_empty()) {
                        out.push(Event::ToolCallArgs { index, delta: a });
                    }
                }
            }
            Some("response.function_call_arguments.delta") => {
                let oi = json
                    .get("output_index")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                if let (Some(index), Some(d)) = (self.tools.get(&oi), str_field(json, "delta")) {
                    out.push(Event::ToolCallArgs {
                        index: *index,
                        delta: d,
                    });
                }
            }
            Some("response.completed") => out.push(Event::Finish(if self.saw_tool {
                StopReason::ToolUse
            } else {
                StopReason::Stop
            })),
            Some("response.incomplete") => {
                let reason = json
                    .pointer("/response/incomplete_details/reason")
                    .and_then(Value::as_str);
                out.push(Event::Finish(if reason == Some("content_filter") {
                    StopReason::ContentFilter
                } else {
                    StopReason::Length
                }));
            }
            _ => {}
        }
        out
    }
}

enum Item {
    Message {
        id: String,
        text: String,
    },
    Reasoning {
        id: String,
        text: String,
    },
    Tool {
        id: String,
        call_id: String,
        name: String,
        args: String,
        index: usize,
    },
}

pub struct Encoder {
    id: String,
    model: String,
    created: i64,
    seq: u64,
    started: bool,
    open: Option<Item>,
    done: Vec<Value>,
    stop: Option<StopReason>,
}

impl Encoder {
    pub fn new(model: String) -> Self {
        Self {
            id: gen_id("resp_"),
            model,
            created: now_secs(),
            seq: 0,
            started: false,
            open: None,
            done: Vec::new(),
            stop: None,
        }
    }

    fn emit(&mut self, name: &str, data: Value) -> Bytes {
        let mut map = Map::new();
        map.insert("type".into(), json!(name));
        map.insert("sequence_number".into(), json!(self.seq));
        if let Value::Object(fields) = data {
            map.extend(fields);
        }
        self.seq += 1;
        sse::encode(Some(name), &Value::Object(map))
    }

    fn ensure_started(&mut self, out: &mut Vec<Bytes>) {
        if self.started {
            return;
        }
        self.started = true;
        let resp = response_object(&self.id, &self.model, self.created, None, vec![], None);
        out.push(self.emit("response.created", json!({"response": resp.clone()})));
        out.push(self.emit("response.in_progress", json!({"response": resp})));
    }

    fn output_index(&self) -> usize {
        self.done.len()
    }

    fn close(&mut self, out: &mut Vec<Bytes>) {
        let Some(item) = self.open.take() else {
            return;
        };
        let oi = self.output_index();
        let final_item = match item {
            Item::Message { id, text } => {
                out.push(self.emit(
                    "response.output_text.done",
                    json!({"item_id": id, "output_index": oi, "content_index": 0, "text": text}),
                ));
                let part = json!({"type": "output_text", "text": text, "annotations": []});
                out.push(self.emit(
                    "response.content_part.done",
                    json!({"item_id": id, "output_index": oi, "content_index": 0, "part": part}),
                ));
                json!({"type": "message", "id": id, "status": "completed", "role": "assistant",
                       "content": [part]})
            }
            Item::Reasoning { id, text } => {
                out.push(self.emit(
                    "response.reasoning_summary_text.done",
                    json!({"item_id": id, "output_index": oi, "summary_index": 0, "text": text}),
                ));
                let part = json!({"type": "summary_text", "text": text});
                out.push(self.emit(
                    "response.reasoning_summary_part.done",
                    json!({"item_id": id, "output_index": oi, "summary_index": 0, "part": part}),
                ));
                json!({"type": "reasoning", "id": id, "summary": [part]})
            }
            Item::Tool {
                id,
                call_id,
                name,
                args,
                ..
            } => {
                out.push(self.emit(
                    "response.function_call_arguments.done",
                    json!({"item_id": id, "output_index": oi, "arguments": args}),
                ));
                json!({"type": "function_call", "id": id, "call_id": call_id, "name": name,
                       "arguments": args, "status": "completed"})
            }
        };
        out.push(self.emit(
            "response.output_item.done",
            json!({"output_index": oi, "item": final_item.clone()}),
        ));
        self.done.push(final_item);
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
            Event::Start { .. } | Event::ReasoningSignature(_) => {}
            Event::Text(t) => {
                if !matches!(self.open, Some(Item::Message { .. })) {
                    self.close(&mut out);
                    let id = gen_id("msg_");
                    let oi = self.output_index();
                    out.push(self.emit(
                        "response.output_item.added",
                        json!({"output_index": oi, "item": {"type": "message", "id": id,
                            "status": "in_progress", "role": "assistant", "content": []}}),
                    ));
                    out.push(self.emit(
                        "response.content_part.added",
                        json!({"item_id": id, "output_index": oi, "content_index": 0,
                            "part": {"type": "output_text", "text": "", "annotations": []}}),
                    ));
                    self.open = Some(Item::Message {
                        id,
                        text: String::new(),
                    });
                }
                let oi = self.output_index();
                let id = match &mut self.open {
                    Some(Item::Message { id, text }) => {
                        text.push_str(&t);
                        id.clone()
                    }
                    _ => unreachable!(),
                };
                out.push(self.emit(
                    "response.output_text.delta",
                    json!({"item_id": id, "output_index": oi, "content_index": 0, "delta": t}),
                ));
            }
            Event::Reasoning(t) => {
                if !matches!(self.open, Some(Item::Reasoning { .. })) {
                    self.close(&mut out);
                    let id = gen_id("rs_");
                    let oi = self.output_index();
                    out.push(self.emit(
                        "response.output_item.added",
                        json!({"output_index": oi, "item": {"type": "reasoning", "id": id, "summary": []}}),
                    ));
                    out.push(self.emit(
                        "response.reasoning_summary_part.added",
                        json!({"item_id": id, "output_index": oi, "summary_index": 0,
                            "part": {"type": "summary_text", "text": ""}}),
                    ));
                    self.open = Some(Item::Reasoning {
                        id,
                        text: String::new(),
                    });
                }
                let oi = self.output_index();
                let id = match &mut self.open {
                    Some(Item::Reasoning { id, text }) => {
                        text.push_str(&t);
                        id.clone()
                    }
                    _ => unreachable!(),
                };
                out.push(self.emit(
                    "response.reasoning_summary_text.delta",
                    json!({"item_id": id, "output_index": oi, "summary_index": 0, "delta": t}),
                ));
            }
            Event::ToolCallStart { index, id, name } => {
                self.close(&mut out);
                let item_id = gen_id("fc_");
                let oi = self.output_index();
                out.push(self.emit(
                    "response.output_item.added",
                    json!({"output_index": oi, "item": {"type": "function_call", "id": item_id,
                        "call_id": id, "name": name, "arguments": "", "status": "in_progress"}}),
                ));
                self.open = Some(Item::Tool {
                    id: item_id,
                    call_id: id,
                    name,
                    args: String::new(),
                    index,
                });
            }
            Event::ToolCallArgs { index, delta } => {
                let oi = self.output_index();
                let item_id = match &mut self.open {
                    Some(Item::Tool {
                        id,
                        args,
                        index: open_index,
                        ..
                    }) if *open_index == index => {
                        args.push_str(&delta);
                        Some(id.clone())
                    }
                    _ => None,
                };
                if let Some(item_id) = item_id {
                    out.push(self.emit(
                        "response.function_call_arguments.delta",
                        json!({"item_id": item_id, "output_index": oi, "delta": delta}),
                    ));
                }
            }
            Event::Finish(s) => self.stop = Some(s),
        }
        out
    }

    pub fn finish(&mut self, usage: Option<&Usage>, render: &dyn UsageAdapter) -> Vec<Bytes> {
        let mut out = Vec::new();
        self.ensure_started(&mut out);
        self.close(&mut out);
        let stop = self.stop.unwrap_or(StopReason::Stop);
        let resp = response_object(
            &self.id,
            &self.model,
            self.created,
            Some(stop),
            self.done.clone(),
            usage.map(|u| render.render(u)),
        );
        let name = if resp["status"] == "incomplete" {
            "response.incomplete"
        } else {
            "response.completed"
        };
        out.push(self.emit(name, json!({"response": resp})));
        out
    }
}
