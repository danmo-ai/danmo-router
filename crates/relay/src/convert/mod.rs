//! Conversion through the IR. Each conversational protocol has one base codec and each dialect
//! layers its spec on top (see `crate::dialect`); any pair of dialects is a decode followed by
//! an encode. Converters never look at model names and never change reasoning settings beyond
//! mapping their representation. Unknown fields survive conversions within one protocol.

mod chat;
mod gemini;
pub mod ir;
mod messages;
mod responses;

use crate::collect::UsageCollector;
use crate::dialect::DialectCodec;
use crate::sse::SseEvent;
use bytes::Bytes;
use danmo_core::dialect::Dialect;
use danmo_core::protocol::Protocol;
use danmo_core::usage::{match_usage, Usage, UsageAdapter};
use serde_json::Value;
use tracing::debug;

/// Request fields the base codec of `protocol` reads.
fn base_keys(protocol: Protocol) -> &'static [&'static str] {
    match protocol {
        Protocol::OpenaiChat => chat::REQUEST_KEYS,
        Protocol::OpenaiResponses => responses::REQUEST_KEYS,
        Protocol::AnthropicMessages => messages::REQUEST_KEYS,
        Protocol::Gemini => gemini::REQUEST_KEYS,
        Protocol::Embeddings => &[],
    }
}

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

/// Decode `body` in dialect `from`, encode it in dialect `to`.
pub fn request(
    from: &dyn DialectCodec,
    to: &dyn DialectCodec,
    body: &Value,
    upstream_model: &str,
    stream: bool,
) -> Result<Value, String> {
    let (fp, tp) = (from.spec().protocol, to.spec().protocol);
    let mut req = decode_request(fp, body)?;
    from.read_request(body, &mut req);
    req.stream = stream;
    let known: Vec<&str> = base_keys(fp)
        .iter()
        .chain(from.request_keys())
        .copied()
        .collect();
    let unknown = ir::unknown_fields(body, &known);
    if fp == tp {
        req.extra = unknown;
    } else {
        if !unknown.is_empty() {
            debug!(
                from = %from.spec().id(),
                to = %to.spec().id(),
                dropped = ?unknown.keys().collect::<Vec<_>>(),
                "fields without a counterpart dropped"
            );
        }
        for m in &mut req.messages {
            m.extra.clear();
        }
    }
    let mut out = encode_request(tp, &req, upstream_model)?;
    let obj = out
        .as_object_mut()
        .ok_or("encoded request is not an object")?;
    ir::insert_missing(obj, &req.extra);
    to.write_request(&req, obj);
    Ok(out)
}

/// Decode an upstream response in dialect `from`, encode it for a client of dialect `to`.
pub fn response(
    from: &dyn DialectCodec,
    to: &dyn DialectCodec,
    body: &Value,
    usage: &Usage,
    model: &str,
) -> Result<Value, String> {
    let (fp, tp) = (from.spec().protocol, to.spec().protocol);
    let render = to.spec().render_for(&usage.adapter);
    let mut resp = match fp {
        Protocol::OpenaiChat => chat::decode_response(body),
        Protocol::OpenaiResponses => responses::decode_response(body),
        Protocol::AnthropicMessages => messages::decode_response(body),
        Protocol::Gemini => gemini::decode_response(body),
        Protocol::Embeddings => Err("embeddings cannot be converted".into()),
    }?;
    if fp == tp {
        for k in from.response_keys() {
            resp.extra.remove(*k);
        }
    } else {
        resp.extra.clear();
        resp.message_extra.clear();
    }
    let mut out = match tp {
        Protocol::OpenaiChat => chat::encode_response(&resp, usage, render, model),
        Protocol::OpenaiResponses => responses::encode_response(&resp, usage, render, model),
        Protocol::AnthropicMessages => messages::encode_response(&resp, usage, render, model),
        Protocol::Gemini => gemini::encode_response(&resp, usage, render, model),
        Protocol::Embeddings => return Err("embeddings cannot be converted".into()),
    };
    if let Some(obj) = out.as_object_mut() {
        ir::insert_missing(obj, &resp.extra);
    }
    Ok(out)
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
    client: &'static Dialect,
}

impl StreamConverter {
    pub fn new(
        from: &'static dyn DialectCodec,
        to: &'static dyn DialectCodec,
        model: String,
        include_usage: bool,
    ) -> Self {
        let upstream = from.spec().protocol;
        Self {
            upstream,
            decoder: Decoder::new(upstream),
            encoder: Encoder::new(to.spec().protocol, model, include_usage),
            collector: Some(UsageCollector::new(from)),
            client: to.spec(),
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
        let render = self
            .client
            .render_for(usage.as_ref().map_or("", |u| u.adapter.as_str()));
        self.encoder.finish(usage.as_ref(), render)
    }
}

#[cfg(test)]
mod tests;
