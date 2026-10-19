//! Intermediate representation shared by all conversational protocols. It only keeps the
//! semantics every protocol has; anything else is dropped when converting.

use serde_json::{json, Map, Value};

#[derive(Debug, Clone, Default)]
pub struct Request {
    pub system: Vec<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<Tool>,
    pub tool_choice: Option<ToolChoice>,
    pub max_tokens: Option<i64>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub stop: Vec<String>,
    pub stream: bool,
    pub reasoning: Option<Reasoning>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text(String),
    /// `url` is either a remote URL or a `data:<mime>;base64,<data>` URL.
    Image {
        url: String,
    },
    Reasoning {
        text: String,
        signature: Option<String>,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
    ToolResult {
        id: String,
        name: Option<String>,
        content: String,
        is_error: bool,
    },
}

#[derive(Debug, Clone)]
pub struct Tool {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolChoice {
    Auto,
    None,
    Required,
    Named(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reasoning {
    pub effort: Option<String>,
    pub budget_tokens: Option<i64>,
}

impl Reasoning {
    pub fn effort(&self) -> String {
        self.effort
            .clone()
            .unwrap_or_else(|| match self.budget_tokens.unwrap_or(8192) {
                b if b <= 4096 => "low".into(),
                b if b <= 16384 => "medium".into(),
                _ => "high".into(),
            })
    }

    pub fn budget(&self) -> i64 {
        let from_effort = match self.effort.as_deref() {
            Some("minimal") => 1024,
            Some("low") => 2048,
            Some("high") => 24576,
            _ => 8192,
        };
        self.budget_tokens.unwrap_or(from_effort)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Stop,
    Length,
    ToolUse,
    ContentFilter,
}

#[derive(Debug, Clone)]
pub struct Response {
    pub id: String,
    pub parts: Vec<Part>,
    pub stop: StopReason,
}

impl Response {
    pub fn has_tool_calls(&self) -> bool {
        self.parts
            .iter()
            .any(|p| matches!(p, Part::ToolCall { .. }))
    }
}

/// Stream events. Tool calls are numbered from 0 in the order they start.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Start {
        id: String,
        model: String,
    },
    Text(String),
    Reasoning(String),
    ReasoningSignature(String),
    ToolCallStart {
        index: usize,
        id: String,
        name: String,
    },
    ToolCallArgs {
        index: usize,
        delta: String,
    },
    Finish(StopReason),
}

pub fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| {
                p.as_str()
                    .map(str::to_string)
                    .or_else(|| p.get("text").and_then(Value::as_str).map(str::to_string))
            })
            .collect::<Vec<_>>()
            .join(""),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

pub fn args_value(arguments: &str) -> Value {
    match serde_json::from_str::<Value>(arguments) {
        Ok(v @ Value::Object(_)) => v,
        _ => json!({}),
    }
}

/// Tool parameter schema accepted by every protocol: an object schema, and never
/// `"required": null` (several upstreams reject it).
pub fn clean_schema(schema: &Value) -> Value {
    fn strip_null_required(v: &mut Value) {
        match v {
            Value::Object(map) => {
                if map.get("required").is_some_and(Value::is_null) {
                    map.remove("required");
                }
                for child in map.values_mut() {
                    strip_null_required(child);
                }
            }
            Value::Array(items) => items.iter_mut().for_each(strip_null_required),
            _ => {}
        }
    }
    let mut schema = match schema {
        Value::Object(_) => schema.clone(),
        _ => json!({}),
    };
    strip_null_required(&mut schema);
    let map = schema.as_object_mut().expect("schema is an object");
    map.entry("type").or_insert_with(|| json!("object"));
    if map.get("type") == Some(&json!("object")) {
        map.entry("properties")
            .or_insert_with(|| Value::Object(Map::new()));
    }
    schema
}

pub fn split_data_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let mime = meta.strip_suffix(";base64")?;
    Some((mime.to_string(), data.to_string()))
}

pub fn data_url(mime: &str, data: &str) -> String {
    format!("data:{mime};base64,{data}")
}

/// Append to the last message when it has the same role; protocols that require alternating
/// roles get merged turns for free.
pub fn push_parts(messages: &mut Vec<Message>, role: Role, parts: Vec<Part>) {
    if parts.is_empty() {
        return;
    }
    match messages.last_mut() {
        Some(last) if last.role == role => last.parts.extend(parts),
        _ => messages.push(Message { role, parts }),
    }
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn gen_id(prefix: &str) -> String {
    format!("{prefix}{}", uuid_like())
}

fn uuid_like() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}{:04x}", t, n & 0xffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_without_required_never_gets_null() {
        let cleaned = clean_schema(&json!({"type": "object", "required": null,
            "properties": {"a": {"type": "object", "required": null}}}));
        assert!(cleaned.get("required").is_none());
        assert!(cleaned["properties"]["a"].get("required").is_none());
        let empty = clean_schema(&Value::Null);
        assert_eq!(empty, json!({"type": "object", "properties": {}}));
    }

    #[test]
    fn data_urls_roundtrip() {
        let url = data_url("image/png", "AAAA");
        assert_eq!(
            split_data_url(&url),
            Some(("image/png".into(), "AAAA".into()))
        );
        assert!(split_data_url("https://x/y.png").is_none());
    }
}
