//! Reads the raw usage object out of upstream responses, per upstream protocol. Never modifies
//! anything; field-set matching happens afterwards in `danmo_core::usage`.

use crate::dialect::DialectCodec;
use danmo_core::protocol::Protocol;
use serde_json::{Map, Value};

fn non_null(v: Option<&Value>) -> Option<&Value> {
    v.filter(|v| !v.is_null())
}

/// Raw usage of a buffered (non-streaming) response body.
pub fn from_body(protocol: Protocol, body: &Value) -> Option<Value> {
    match protocol {
        Protocol::Gemini => non_null(body.get("usageMetadata")).cloned(),
        Protocol::OpenaiResponses => non_null(body.get("usage"))
            .or_else(|| non_null(body.pointer("/response/usage")))
            .cloned(),
        _ => non_null(body.get("usage")).cloned(),
    }
}

/// Accumulates usage across stream events of one upstream dialect.
pub struct UsageCollector {
    protocol: Protocol,
    dialect: &'static dyn DialectCodec,
    usage: Option<Value>,
    pub soft_error: Option<String>,
}

impl UsageCollector {
    pub fn new(dialect: &'static dyn DialectCodec) -> Self {
        Self {
            protocol: dialect.spec().protocol,
            dialect,
            usage: None,
            soft_error: None,
        }
    }

    pub fn observe(&mut self, kind: Option<&str>, json: &Value) {
        if let Some(err) = self.dialect.soft_error(json) {
            self.soft_error = Some(err);
        }
        match self.protocol {
            Protocol::OpenaiChat | Protocol::Embeddings => {
                if let Some(u) = non_null(json.get("usage")) {
                    self.usage = Some(u.clone());
                }
            }
            Protocol::OpenaiResponses => {
                if matches!(
                    kind,
                    Some("response.completed" | "response.incomplete" | "response.failed")
                ) {
                    if let Some(u) = non_null(json.pointer("/response/usage")) {
                        self.usage = Some(u.clone());
                    }
                }
            }
            Protocol::AnthropicMessages => match kind {
                Some("message_start") => {
                    if let Some(u) = non_null(json.pointer("/message/usage")) {
                        self.merge(u);
                    }
                }
                Some("message_delta") => {
                    if let Some(u) = non_null(json.get("usage")) {
                        self.merge(u);
                    }
                }
                _ => {}
            },
            Protocol::Gemini => {
                if let Some(u) = non_null(json.get("usageMetadata")) {
                    self.usage = Some(u.clone());
                }
            }
        }
    }

    /// `message_delta` usage is cumulative and may repeat or update `message_start` fields.
    fn merge(&mut self, delta: &Value) {
        let Some(delta) = delta.as_object() else {
            return;
        };
        let current = self
            .usage
            .get_or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("usage accumulator is an object");
        for (k, v) in delta {
            if !v.is_null() {
                current.insert(k.clone(), v.clone());
            }
        }
    }

    pub fn finish(self) -> Option<Value> {
        self.usage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialect;
    use danmo_core::dialect::{find, standard};
    use serde_json::json;

    #[test]
    fn messages_merges_start_and_last_delta() {
        let mut c = UsageCollector::new(dialect::codec(standard(Protocol::AnthropicMessages)));
        c.observe(
            Some("message_start"),
            &json!({"message": {"usage": {"input_tokens": 10, "cache_read_input_tokens": 90, "output_tokens": 1}}}),
        );
        c.observe(
            Some("message_delta"),
            &json!({"usage": {"output_tokens": 5}}),
        );
        c.observe(
            Some("message_delta"),
            &json!({"usage": {"output_tokens": 42}}),
        );
        let u = c.finish().unwrap();
        assert_eq!(u["input_tokens"], 10);
        assert_eq!(u["cache_read_input_tokens"], 90);
        assert_eq!(u["output_tokens"], 42);
    }

    #[test]
    fn soft_errors_only_for_dialects_that_report_them() {
        let body =
            json!({"base_resp": {"status_code": 1008, "status_msg": "insufficient balance"}});
        let chat = |name| dialect::codec(find(Protocol::OpenaiChat, name).unwrap());
        assert!(chat("minimax").soft_error(&body).unwrap().contains("1008"));
        assert!(chat("opaque").soft_error(&body).is_some());
        assert!(chat("standard").soft_error(&body).is_none());
        assert!(chat("minimax")
            .soft_error(&json!({"base_resp": {"status_code": 0}}))
            .is_none());
    }
}
