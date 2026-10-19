//! Protocol dialects.
//!
//! A dialect is one vendor's full wire spec of a protocol. The inbound dialect of a request is
//! the model's native dialect on that protocol (`standard` when unset); the upstream dialect is
//! the one configured on the channel's model row. The relay crate implements the codecs; this
//! registry only names them and lists their usage field sets.

use crate::error::{AppError, AppResult};
use crate::protocol::Protocol;
use crate::usage::{self, UsageAdapter};
use serde::Serialize;
use std::collections::HashMap;

pub const STANDARD: &str = "standard";
pub const OPAQUE: &str = "opaque";

#[derive(Debug, Serialize)]
pub struct Dialect {
    pub protocol: Protocol,
    pub name: &'static str,
    /// Usage field sets this spec reports; the first renders usage for clients of the dialect.
    pub usage: &'static [&'static str],
    /// Unknown spec, e.g. an aggregator: only same-protocol passthrough, usage by feature match.
    pub opaque: bool,
    pub label: &'static str,
}

const fn d(
    protocol: Protocol,
    name: &'static str,
    usage: &'static [&'static str],
    label: &'static str,
) -> Dialect {
    Dialect {
        protocol,
        name,
        usage,
        opaque: false,
        label,
    }
}

const fn opaque(protocol: Protocol, usage: &'static [&'static str]) -> Dialect {
    Dialect {
        protocol,
        name: OPAQUE,
        usage,
        opaque: true,
        label: "Unknown spec (aggregator), passthrough only",
    }
}

use Protocol::*;

pub static DIALECTS: &[Dialect] = &[
    d(OpenaiChat, STANDARD, &["openai_chat"], "OpenAI Chat"),
    opaque(OpenaiChat, &["openai_chat"]),
    d(
        OpenaiChat,
        "thinking",
        &["deepseek_chat", "openai_chat"],
        "DeepSeek / Ark / GLM: thinking.type + reasoning_effort",
    ),
    d(
        OpenaiChat,
        "enable_thinking",
        &["openai_chat"],
        "Bailian: enable_thinking + thinking_budget",
    ),
    d(
        OpenaiChat,
        "kimi",
        &["kimi_chat", "openai_chat"],
        "Kimi: thinking.type + reasoning_effort",
    ),
    d(
        OpenaiChat,
        "qianfan",
        &["qianfan_chat", "openai_chat"],
        "Qianfan DeepSeek/GLM/Kimi: thinking.type + thinking_budget",
    ),
    d(
        OpenaiChat,
        "qianfan_ernie",
        &["qianfan_chat", "openai_chat"],
        "Qianfan ERNIE/Qwen3: enable_thinking + thinking_strategy",
    ),
    d(
        OpenaiChat,
        "hunyuan",
        &["openai_chat"],
        "Hunyuan: enable_enhancement / search_info / citation",
    ),
    d(
        OpenaiChat,
        "minimax",
        &["openai_chat"],
        "MiniMax: thinking adaptive, reasoning_split, base_resp errors",
    ),
    d(
        OpenaiResponses,
        STANDARD,
        &["openai_responses"],
        "OpenAI Responses",
    ),
    opaque(OpenaiResponses, &["openai_responses"]),
    d(
        OpenaiResponses,
        "thinking",
        &["openai_responses"],
        "Bailian / Ark: thinking.type + reasoning.effort",
    ),
    d(
        AnthropicMessages,
        STANDARD,
        &["anthropic_messages"],
        "Anthropic Messages",
    ),
    opaque(AnthropicMessages, &["anthropic_messages"]),
    d(
        AnthropicMessages,
        "effort",
        &["anthropic_messages"],
        "MiniMax Anthropic: output_config.effort, adaptive thinking",
    ),
    d(Gemini, STANDARD, &["gemini"], "Gemini"),
    d(Embeddings, STANDARD, &["embeddings"], "OpenAI Embeddings"),
];

pub fn find(protocol: Protocol, name: &str) -> Option<&'static Dialect> {
    DIALECTS
        .iter()
        .find(|d| d.protocol == protocol && d.name == name)
}

pub fn of(protocol: Protocol) -> impl Iterator<Item = &'static Dialect> {
    DIALECTS.iter().filter(move |d| d.protocol == protocol)
}

pub fn standard(protocol: Protocol) -> &'static Dialect {
    find(protocol, STANDARD).expect("every protocol has a standard dialect")
}

/// Unknown names fall back to `standard`; stored values are validated on write.
pub fn resolve(protocol: Protocol, name: &str) -> &'static Dialect {
    find(protocol, name).unwrap_or_else(|| standard(protocol))
}

pub fn validate(protocol: Protocol, name: &str) -> AppResult<&'static Dialect> {
    find(protocol, name).ok_or_else(|| {
        AppError::BadRequest(format!("unknown dialect '{name}' for protocol {protocol}"))
    })
}

impl Dialect {
    pub fn id(&self) -> String {
        format!("{}/{}", self.protocol, self.name)
    }

    pub fn is_standard(&self) -> bool {
        self.name == STANDARD
    }

    /// Usage adapter for rendering to clients of this dialect: the parsed field set when this
    /// spec reports it, otherwise the spec's primary one.
    pub fn render_for(&self, parsed: &str) -> &'static dyn UsageAdapter {
        let name = if self.usage.contains(&parsed) {
            parsed
        } else {
            self.usage[0]
        };
        usage::adapter(name).unwrap_or_else(|| usage::default_adapter(self.protocol))
    }
}

/// Native dialects of a model, keyed by protocol.
#[derive(Debug, Clone, Default)]
pub struct NativeDialects(HashMap<Protocol, &'static Dialect>);

impl NativeDialects {
    pub fn parse(raw: &str) -> AppResult<Self> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Ok(Self::default());
        }
        let map: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(raw).map_err(|e| {
                AppError::BadRequest(format!("native_dialects must be a JSON object: {e}"))
            })?;
        let mut out = HashMap::new();
        for (k, v) in map {
            let protocol = Protocol::parse(&k).ok_or_else(|| {
                AppError::BadRequest(format!("native_dialects: unknown protocol '{k}'"))
            })?;
            let name = v.as_str().ok_or_else(|| {
                AppError::BadRequest(format!("native_dialects.{k} must be a string"))
            })?;
            if name.is_empty() {
                continue;
            }
            let dialect = validate(protocol, name)?;
            if dialect.opaque {
                return Err(AppError::BadRequest(
                    "opaque is a channel dialect, not a native one".into(),
                ));
            }
            out.insert(protocol, dialect);
        }
        Ok(Self(out))
    }

    /// Lenient read of a stored value.
    pub fn load(raw: &str) -> Self {
        Self::parse(raw).unwrap_or_default()
    }

    pub fn configured(&self, protocol: Protocol) -> Option<&'static Dialect> {
        self.0.get(&protocol).copied()
    }

    pub fn get(&self, protocol: Protocol) -> &'static Dialect {
        self.configured(protocol)
            .unwrap_or_else(|| standard(protocol))
    }

    pub fn to_json(&self) -> String {
        if self.0.is_empty() {
            return String::new();
        }
        let map: std::collections::BTreeMap<&str, &str> =
            self.0.iter().map(|(p, d)| (p.as_str(), d.name)).collect();
        serde_json::to_string(&map).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_protocol_has_standard_and_valid_usage() {
        for p in Protocol::ALL {
            assert!(standard(p).is_standard());
        }
        for d in DIALECTS {
            assert!(!d.usage.is_empty(), "{}", d.id());
            for u in d.usage {
                assert!(usage::adapter(u).is_some(), "{} lists unknown {u}", d.id());
            }
        }
    }

    #[test]
    fn render_prefers_parsed_set_within_spec() {
        let thinking = find(OpenaiChat, "thinking").unwrap();
        assert_eq!(thinking.render_for("deepseek_chat").name(), "deepseek_chat");
        assert_eq!(thinking.render_for("openai_chat").name(), "openai_chat");
        assert_eq!(thinking.render_for("qianfan_chat").name(), "deepseek_chat");
        assert_eq!(
            standard(OpenaiChat).render_for("qianfan_chat").name(),
            "openai_chat"
        );
    }

    #[test]
    fn native_dialects_parse_and_validate() {
        let n =
            NativeDialects::parse(r#"{"openai_chat":"thinking","anthropic_messages":"standard"}"#)
                .unwrap();
        assert_eq!(n.get(OpenaiChat).name, "thinking");
        assert_eq!(n.get(Gemini).name, STANDARD);
        assert!(n.configured(Gemini).is_none());
        assert!(NativeDialects::parse(r#"{"openai_chat":"nope"}"#).is_err());
        assert!(NativeDialects::parse(r#"{"openai_chat":"opaque"}"#).is_err());
        assert!(NativeDialects::parse(r#"{"gemini":"thinking"}"#).is_err());
        assert!(NativeDialects::parse("").unwrap().to_json().is_empty());
    }
}
