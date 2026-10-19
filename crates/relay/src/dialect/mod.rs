//! Dialect codecs. Each dialect implements one vendor's spec of a protocol on top of that
//! protocol's base codec in `convert`: it names the request fields the spec defines beyond the
//! base, reads them into the IR, and writes the IR back in the spec's own fields. A vendor
//! changing its spec only touches its own dialect.

mod chat;
mod messages;
mod responses;

use crate::convert::ir;
use danmo_core::dialect::{self as registry, Dialect};
use danmo_core::protocol::Protocol;
use serde_json::{Map, Value};

pub trait DialectCodec: Sync {
    fn spec(&self) -> &'static Dialect;

    /// Top-level request fields this spec defines beyond the protocol's base codec. They are
    /// read by `read_request` and never carried to another dialect as unknown fields.
    fn request_keys(&self) -> &'static [&'static str] {
        &[]
    }

    /// Top-level response fields that only make sense to clients of this spec.
    fn response_keys(&self) -> &'static [&'static str] {
        &[]
    }

    /// Runs after the base decode.
    fn read_request(&self, _body: &Value, _req: &mut ir::Request) {}

    /// Runs after the base encode and unknown-field restore; owns the fields it writes.
    fn write_request(&self, _req: &ir::Request, _out: &mut Map<String, Value>) {}

    /// Vendors that report failures inside an HTTP 200 body.
    fn soft_error(&self, _body: &Value) -> Option<String> {
        None
    }
}

/// A dialect that is exactly its protocol's base codec.
struct Plain(Protocol, &'static str);

impl DialectCodec for Plain {
    fn spec(&self) -> &'static Dialect {
        spec(self.0, self.1)
    }

    fn soft_error(&self, body: &Value) -> Option<String> {
        // Opaque upstreams may be anything, including MiniMax behind an aggregator.
        if self.1 == registry::OPAQUE {
            base_resp_error(body)
        } else {
            None
        }
    }
}

fn spec(protocol: Protocol, name: &str) -> &'static Dialect {
    registry::find(protocol, name).expect("codec names a registered dialect")
}

static CHAT_STANDARD: Plain = Plain(Protocol::OpenaiChat, registry::STANDARD);
static CHAT_OPAQUE: Plain = Plain(Protocol::OpenaiChat, registry::OPAQUE);
static RESPONSES_STANDARD: Plain = Plain(Protocol::OpenaiResponses, registry::STANDARD);
static RESPONSES_OPAQUE: Plain = Plain(Protocol::OpenaiResponses, registry::OPAQUE);
static MESSAGES_STANDARD: Plain = Plain(Protocol::AnthropicMessages, registry::STANDARD);
static MESSAGES_OPAQUE: Plain = Plain(Protocol::AnthropicMessages, registry::OPAQUE);
static GEMINI_STANDARD: Plain = Plain(Protocol::Gemini, registry::STANDARD);
static EMBEDDINGS_STANDARD: Plain = Plain(Protocol::Embeddings, registry::STANDARD);

/// Codec of a registered dialect.
pub fn codec(d: &Dialect) -> &'static dyn DialectCodec {
    use Protocol::*;
    match (d.protocol, d.name) {
        (OpenaiChat, "opaque") => &CHAT_OPAQUE,
        (OpenaiChat, "thinking") => &chat::Thinking,
        (OpenaiChat, "enable_thinking") => &chat::EnableThinking,
        (OpenaiChat, "kimi") => &chat::Kimi,
        (OpenaiChat, "qianfan") => &chat::Qianfan,
        (OpenaiChat, "qianfan_ernie") => &chat::QianfanErnie,
        (OpenaiChat, "hunyuan") => &chat::Hunyuan,
        (OpenaiChat, "minimax") => &chat::Minimax,
        (OpenaiChat, _) => &CHAT_STANDARD,
        (OpenaiResponses, "opaque") => &RESPONSES_OPAQUE,
        (OpenaiResponses, "thinking") => &responses::Thinking,
        (OpenaiResponses, _) => &RESPONSES_STANDARD,
        (AnthropicMessages, "opaque") => &MESSAGES_OPAQUE,
        (AnthropicMessages, "effort") => &messages::Effort,
        (AnthropicMessages, _) => &MESSAGES_STANDARD,
        (Gemini, _) => &GEMINI_STANDARD,
        (Embeddings, _) => &EMBEDDINGS_STANDARD,
    }
}

/// MiniMax `base_resp`: a non-zero status code is a failure even under HTTP 200.
pub fn base_resp_error(body: &Value) -> Option<String> {
    let base = body.get("base_resp")?;
    let code = base.get("status_code").and_then(Value::as_i64)?;
    if code == 0 {
        return None;
    }
    let msg = base
        .get("status_msg")
        .and_then(Value::as_str)
        .unwrap_or("upstream error");
    Some(format!("upstream error {code}: {msg}"))
}

/// `thinking.type` as used by DeepSeek, Ark, GLM, Kimi, Qianfan and MiniMax.
fn read_thinking_type(body: &Value, req: &mut ir::Request) {
    match body.pointer("/thinking/type").and_then(Value::as_str) {
        Some("enabled") | Some("adaptive") => {
            req.reasoning.get_or_insert_with(Default::default).enabled = Some(true)
        }
        Some("disabled") => req.reasoning = Some(ir::Reasoning::off()),
        Some(_) => {
            req.reasoning.get_or_insert_with(Default::default);
        }
        None => {}
    }
}

fn read_budget(body: &Value, key: &str, req: &mut ir::Request) {
    if let Some(b) = body.get(key).and_then(Value::as_i64) {
        let r = req.reasoning.get_or_insert_with(Default::default);
        if !r.is_off() {
            r.budget_tokens = Some(b);
        }
    }
}

fn read_enable_flag(body: &Value, key: &str, req: &mut ir::Request) {
    match body.get(key).and_then(Value::as_bool) {
        Some(true) => req.reasoning.get_or_insert_with(Default::default).enabled = Some(true),
        Some(false) => req.reasoning = Some(ir::Reasoning::off()),
        None => {}
    }
}

/// Reasoning the request wants on, as opposed to off or left to the upstream default.
fn wants_on(req: &ir::Request) -> Option<&ir::Reasoning> {
    req.reasoning
        .as_ref()
        .filter(|r| !r.is_off() && (r.is_on() || r.has_level()))
}

fn is_off(req: &ir::Request) -> bool {
    req.reasoning.as_ref().is_some_and(ir::Reasoning::is_off)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_dialect_has_its_codec() {
        for d in registry::DIALECTS {
            assert_eq!(codec(d).spec().id(), d.id());
        }
    }
}
