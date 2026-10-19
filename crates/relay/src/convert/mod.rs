use crate::sse::SseEvent;
use bytes::Bytes;
use danmo_core::protocol::Protocol;
use danmo_core::usage::{Usage, UsageAdapter};
use serde_json::Value;

pub fn request(
    from: Protocol,
    to: Protocol,
    _body: &Value,
    _upstream_model: &str,
    _stream: bool,
) -> Result<Value, String> {
    Err(format!("conversion {from} -> {to} is not available"))
}

pub fn response(
    from: Protocol,
    to: Protocol,
    _body: &Value,
    _usage: &Usage,
    _render: &'static dyn UsageAdapter,
    _model: &str,
) -> Result<Value, String> {
    Err(format!("conversion {from} -> {to} is not available"))
}

pub struct StreamConverter;

impl StreamConverter {
    pub fn new(
        _from: Protocol,
        _to: Protocol,
        _render: &'static dyn UsageAdapter,
        _model: String,
    ) -> Self {
        Self
    }

    pub fn event(&mut self, _ev: &SseEvent, _json: Option<&Value>) -> Vec<Bytes> {
        Vec::new()
    }

    pub fn finish(&mut self) -> Vec<Bytes> {
        Vec::new()
    }
}
