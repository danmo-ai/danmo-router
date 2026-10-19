//! Protocol conversion through the IR. Each conversational protocol has one codec; any inbound
//! and upstream pair (12 directions) is a decode followed by an encode. Converters never look
//! at model names and never change reasoning settings beyond mapping their representation.

mod chat;
mod gemini;
pub mod ir;
mod messages;
mod responses;

use crate::collect::UsageCollector;
use crate::sse::SseEvent;
use bytes::Bytes;
use danmo_core::protocol::Protocol;
use danmo_core::usage::{match_usage, Usage, UsageAdapter};
use serde_json::Value;

fn decode_request(protocol: Protocol, body: &Value) -> Result<ir::Request, String> {
    match protocol {
        Protocol::OpenaiChat => chat::decode_request(body),
        Protocol::OpenaiResponses => responses::decode_request(body),
        Protocol::AnthropicMessages => messages::decode_request(body),
        Protocol::Gemini => gemini::decode_request(body),
        Protocol::Embeddings => Err("embeddings cannot be converted".into()),
    }
}

fn encode_request(protocol: Protocol, req: &ir::Request, model: &str) -> Result<Value, String> {
    Ok(match protocol {
        Protocol::OpenaiChat => chat::encode_request(req, model),
        Protocol::OpenaiResponses => responses::encode_request(req, model),
        Protocol::AnthropicMessages => messages::encode_request(req, model),
        Protocol::Gemini => gemini::encode_request(req),
        Protocol::Embeddings => return Err("embeddings cannot be converted".into()),
    })
}

pub fn request(
    from: Protocol,
    to: Protocol,
    body: &Value,
    upstream_model: &str,
    stream: bool,
) -> Result<Value, String> {
    let mut req = decode_request(from, body)?;
    req.stream = stream;
    encode_request(to, &req, upstream_model)
}

pub fn response(
    from: Protocol,
    to: Protocol,
    body: &Value,
    usage: &Usage,
    render: &'static dyn UsageAdapter,
    model: &str,
) -> Result<Value, String> {
    let resp = match from {
        Protocol::OpenaiChat => chat::decode_response(body),
        Protocol::OpenaiResponses => responses::decode_response(body),
        Protocol::AnthropicMessages => messages::decode_response(body),
        Protocol::Gemini => gemini::decode_response(body),
        Protocol::Embeddings => Err("embeddings cannot be converted".into()),
    }?;
    Ok(match to {
        Protocol::OpenaiChat => chat::encode_response(&resp, usage, render, model),
        Protocol::OpenaiResponses => responses::encode_response(&resp, usage, render, model),
        Protocol::AnthropicMessages => messages::encode_response(&resp, usage, render, model),
        Protocol::Gemini => gemini::encode_response(&resp, usage, render, model),
        Protocol::Embeddings => return Err("embeddings cannot be converted".into()),
    })
}

enum Decoder {
    Chat(chat::Decoder),
    Responses(responses::Decoder),
    Messages(messages::Decoder),
    Gemini(gemini::Decoder),
}

impl Decoder {
    fn new(protocol: Protocol) -> Self {
        match protocol {
            Protocol::OpenaiResponses => Decoder::Responses(Default::default()),
            Protocol::AnthropicMessages => Decoder::Messages(Default::default()),
            Protocol::Gemini => Decoder::Gemini(Default::default()),
            _ => Decoder::Chat(Default::default()),
        }
    }

    fn event(&mut self, kind: Option<&str>, json: &Value) -> Vec<ir::Event> {
        match self {
            Decoder::Chat(d) => d.event(json),
            Decoder::Responses(d) => d.event(kind, json),
            Decoder::Messages(d) => d.event(kind, json),
            Decoder::Gemini(d) => d.event(json),
        }
    }
}

enum Encoder {
    Chat(chat::Encoder),
    Responses(responses::Encoder),
    Messages(messages::Encoder),
    Gemini(gemini::Encoder),
}

impl Encoder {
    fn new(protocol: Protocol, model: String, include_usage: bool) -> Self {
        match protocol {
            Protocol::OpenaiResponses => Encoder::Responses(responses::Encoder::new(model)),
            Protocol::AnthropicMessages => Encoder::Messages(messages::Encoder::new(model)),
            Protocol::Gemini => Encoder::Gemini(gemini::Encoder::new(model)),
            _ => Encoder::Chat(chat::Encoder::new(model, include_usage)),
        }
    }

    fn event(&mut self, e: ir::Event) -> Vec<Bytes> {
        match self {
            Encoder::Chat(x) => x.event(e),
            Encoder::Responses(x) => x.event(e),
            Encoder::Messages(x) => x.event(e),
            Encoder::Gemini(x) => x.event(e),
        }
    }

    fn finish(&mut self, usage: Option<&Usage>, render: &dyn UsageAdapter) -> Vec<Bytes> {
        match self {
            Encoder::Chat(x) => x.finish(usage, render),
            Encoder::Responses(x) => x.finish(usage, render),
            Encoder::Messages(x) => x.finish(usage, render),
            Encoder::Gemini(x) => x.finish(usage, render),
        }
    }
}

/// Upstream SSE events in, inbound-protocol SSE events out. Terminal events (finish reason and
/// usage) are emitted once the upstream stream ends, because only then is usage complete.
pub struct StreamConverter {
    upstream: Protocol,
    decoder: Decoder,
    encoder: Encoder,
    collector: Option<UsageCollector>,
    render: &'static dyn UsageAdapter,
}

impl StreamConverter {
    pub fn new(
        from: Protocol,
        to: Protocol,
        render: &'static dyn UsageAdapter,
        model: String,
        include_usage: bool,
    ) -> Self {
        Self {
            upstream: from,
            decoder: Decoder::new(from),
            encoder: Encoder::new(to, model, include_usage),
            collector: Some(UsageCollector::new(from)),
            render,
        }
    }

    pub fn event(&mut self, ev: &SseEvent, json: Option<&Value>) -> Vec<Bytes> {
        let Some(json) = json else {
            return Vec::new();
        };
        let kind = ev.kind(Some(json));
        if let Some(c) = &mut self.collector {
            c.observe(kind, json);
        }
        self.decoder
            .event(kind, json)
            .into_iter()
            .flat_map(|e| self.encoder.event(e))
            .collect()
    }

    pub fn finish(&mut self) -> Vec<Bytes> {
        let usage = self
            .collector
            .take()
            .and_then(UsageCollector::finish)
            .and_then(|raw| match_usage(&raw, self.upstream).ok());
        self.encoder.finish(usage.as_ref(), self.render)
    }
}

#[cfg(test)]
mod tests;
