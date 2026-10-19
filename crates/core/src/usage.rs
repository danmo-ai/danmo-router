//! Usage field sets.
//!
//! Upstreams report usage in a handful of field sets that always appear together. Each set is
//! one adapter; the adapter is chosen by the fields present in the response, never by model,
//! channel or base URL. A response whose usage matches no adapter cannot be billed.

use crate::protocol::Protocol;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// Standard usage. `input_tokens` includes cached tokens; `output_tokens` includes reasoning.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub search_count: i64,
    pub adapter: String,
}

impl Usage {
    pub fn uncached_input_tokens(&self) -> i64 {
        (self.input_tokens - self.cache_read_tokens - self.cache_write_tokens).max(0)
    }

    pub fn total_tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageError(pub String);

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

pub trait UsageAdapter: Sync {
    fn name(&self) -> &'static str;
    /// Protocol whose responses carry this field set; used when re-rendering usage.
    fn protocol(&self) -> Protocol;
    fn matches(&self, raw: &Map<String, Value>, protocol: Protocol) -> bool;
    fn parse(&self, raw: &Map<String, Value>) -> Option<Usage>;
    fn render(&self, usage: &Usage) -> Value;
}

/// Most specific first; the first adapter whose features match decides.
pub static ADAPTERS: &[&dyn UsageAdapter] = &[
    &AnthropicMessages,
    &OpenaiResponses,
    &Gemini,
    &DeepseekChat,
    &QianfanChat,
    &KimiChat,
    &Embeddings,
    &OpenaiChat,
];

pub fn adapter(name: &str) -> Option<&'static dyn UsageAdapter> {
    ADAPTERS.iter().copied().find(|a| a.name() == name)
}

pub fn default_adapter(protocol: Protocol) -> &'static dyn UsageAdapter {
    match protocol {
        Protocol::OpenaiChat => &OpenaiChat,
        Protocol::OpenaiResponses => &OpenaiResponses,
        Protocol::AnthropicMessages => &AnthropicMessages,
        Protocol::Gemini => &Gemini,
        Protocol::Embeddings => &Embeddings,
    }
}

/// Pick the adapter by field features and parse `raw` into standard usage.
pub fn match_usage(raw: &Value, protocol: Protocol) -> Result<Usage, UsageError> {
    let obj = raw
        .as_object()
        .ok_or_else(|| UsageError("usage is not an object".into()))?;
    let adapter = ADAPTERS
        .iter()
        .find(|a| a.matches(obj, protocol))
        .ok_or_else(|| UsageError(format!("no usage adapter matches: {raw}")))?;
    let mut usage = adapter
        .parse(obj)
        .ok_or_else(|| UsageError(format!("{} usage is malformed: {raw}", adapter.name())))?;
    usage.adapter = adapter.name().to_string();
    Ok(usage)
}

fn int(obj: &Map<String, Value>, key: &str) -> Option<i64> {
    let v = obj.get(key)?;
    v.as_i64()
        .or_else(|| v.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64))
        .filter(|n| *n >= 0)
}

fn nested(obj: &Map<String, Value>, outer: &str, key: &str) -> Option<i64> {
    obj.get(outer)
        .and_then(Value::as_object)
        .and_then(|o| int(o, key))
}

fn has(obj: &Map<String, Value>, key: &str) -> bool {
    obj.get(key).is_some_and(|v| !v.is_null())
}

fn has_nested(obj: &Map<String, Value>, outer: &str, key: &str) -> bool {
    obj.get(outer)
        .and_then(Value::as_object)
        .is_some_and(|o| has(o, key))
}

fn chat_base(usage: &Usage) -> Map<String, Value> {
    let mut out = Map::new();
    out.insert("prompt_tokens".into(), json!(usage.input_tokens));
    out.insert("completion_tokens".into(), json!(usage.output_tokens));
    out.insert("total_tokens".into(), json!(usage.total_tokens()));
    out.insert(
        "prompt_tokens_details".into(),
        json!({ "cached_tokens": usage.cache_read_tokens }),
    );
    out.insert(
        "completion_tokens_details".into(),
        json!({ "reasoning_tokens": usage.reasoning_tokens }),
    );
    out
}

fn chat_parse(obj: &Map<String, Value>) -> Option<Usage> {
    Some(Usage {
        input_tokens: int(obj, "prompt_tokens")?,
        output_tokens: int(obj, "completion_tokens").unwrap_or(0),
        cache_read_tokens: nested(obj, "prompt_tokens_details", "cached_tokens").unwrap_or(0),
        reasoning_tokens: nested(obj, "completion_tokens_details", "reasoning_tokens").unwrap_or(0),
        ..Usage::default()
    })
}

/// Anthropic Messages: `input_tokens` excludes cache reads and writes.
pub struct AnthropicMessages;

impl UsageAdapter for AnthropicMessages {
    fn name(&self) -> &'static str {
        "anthropic_messages"
    }
    fn protocol(&self) -> Protocol {
        Protocol::AnthropicMessages
    }
    fn matches(&self, raw: &Map<String, Value>, protocol: Protocol) -> bool {
        has(raw, "cache_read_input_tokens")
            || has(raw, "cache_creation_input_tokens")
            || (protocol == Protocol::AnthropicMessages && has(raw, "input_tokens"))
    }
    fn parse(&self, raw: &Map<String, Value>) -> Option<Usage> {
        let uncached = int(raw, "input_tokens").unwrap_or(0);
        let read = int(raw, "cache_read_input_tokens").unwrap_or(0);
        let write = int(raw, "cache_creation_input_tokens").unwrap_or(0);
        if !has(raw, "input_tokens") && !has(raw, "output_tokens") {
            return None;
        }
        Some(Usage {
            input_tokens: uncached + read + write,
            cache_read_tokens: read,
            cache_write_tokens: write,
            output_tokens: int(raw, "output_tokens").unwrap_or(0),
            search_count: nested(raw, "server_tool_use", "web_search_requests").unwrap_or(0),
            ..Usage::default()
        })
    }
    fn render(&self, usage: &Usage) -> Value {
        json!({
            "input_tokens": usage.uncached_input_tokens(),
            "cache_creation_input_tokens": usage.cache_write_tokens,
            "cache_read_input_tokens": usage.cache_read_tokens,
            "output_tokens": usage.output_tokens,
        })
    }
}

/// OpenAI Responses: `input_tokens` includes cached tokens.
pub struct OpenaiResponses;

impl UsageAdapter for OpenaiResponses {
    fn name(&self) -> &'static str {
        "openai_responses"
    }
    fn protocol(&self) -> Protocol {
        Protocol::OpenaiResponses
    }
    fn matches(&self, raw: &Map<String, Value>, protocol: Protocol) -> bool {
        has(raw, "input_tokens_details")
            || has(raw, "output_tokens_details")
            || (protocol == Protocol::OpenaiResponses && has(raw, "input_tokens"))
    }
    fn parse(&self, raw: &Map<String, Value>) -> Option<Usage> {
        Some(Usage {
            input_tokens: int(raw, "input_tokens")?,
            cache_read_tokens: nested(raw, "input_tokens_details", "cached_tokens").unwrap_or(0),
            output_tokens: int(raw, "output_tokens").unwrap_or(0),
            reasoning_tokens: nested(raw, "output_tokens_details", "reasoning_tokens").unwrap_or(0),
            ..Usage::default()
        })
    }
    fn render(&self, usage: &Usage) -> Value {
        json!({
            "input_tokens": usage.input_tokens,
            "input_tokens_details": { "cached_tokens": usage.cache_read_tokens },
            "output_tokens": usage.output_tokens,
            "output_tokens_details": { "reasoning_tokens": usage.reasoning_tokens },
            "total_tokens": usage.total_tokens(),
        })
    }
}

/// Gemini `usageMetadata`: `candidatesTokenCount` excludes thoughts.
pub struct Gemini;

impl UsageAdapter for Gemini {
    fn name(&self) -> &'static str {
        "gemini"
    }
    fn protocol(&self) -> Protocol {
        Protocol::Gemini
    }
    fn matches(&self, raw: &Map<String, Value>, _protocol: Protocol) -> bool {
        has(raw, "promptTokenCount")
    }
    fn parse(&self, raw: &Map<String, Value>) -> Option<Usage> {
        let thoughts = int(raw, "thoughtsTokenCount").unwrap_or(0);
        Some(Usage {
            input_tokens: int(raw, "promptTokenCount")?
                + int(raw, "toolUsePromptTokenCount").unwrap_or(0),
            cache_read_tokens: int(raw, "cachedContentTokenCount").unwrap_or(0),
            output_tokens: int(raw, "candidatesTokenCount").unwrap_or(0) + thoughts,
            reasoning_tokens: thoughts,
            ..Usage::default()
        })
    }
    fn render(&self, usage: &Usage) -> Value {
        let mut out = Map::new();
        out.insert("promptTokenCount".into(), json!(usage.input_tokens));
        out.insert(
            "candidatesTokenCount".into(),
            json!(usage.output_tokens - usage.reasoning_tokens),
        );
        if usage.reasoning_tokens > 0 {
            out.insert("thoughtsTokenCount".into(), json!(usage.reasoning_tokens));
        }
        if usage.cache_read_tokens > 0 {
            out.insert(
                "cachedContentTokenCount".into(),
                json!(usage.cache_read_tokens),
            );
        }
        out.insert("totalTokenCount".into(), json!(usage.total_tokens()));
        Value::Object(out)
    }
}

/// DeepSeek: `prompt_cache_hit_tokens` and `prompt_cache_miss_tokens` always come as a pair.
pub struct DeepseekChat;

impl UsageAdapter for DeepseekChat {
    fn name(&self) -> &'static str {
        "deepseek_chat"
    }
    fn protocol(&self) -> Protocol {
        Protocol::OpenaiChat
    }
    fn matches(&self, raw: &Map<String, Value>, _protocol: Protocol) -> bool {
        has(raw, "prompt_cache_hit_tokens") && has(raw, "prompt_cache_miss_tokens")
    }
    fn parse(&self, raw: &Map<String, Value>) -> Option<Usage> {
        let hit = int(raw, "prompt_cache_hit_tokens")?;
        let miss = int(raw, "prompt_cache_miss_tokens")?;
        Some(Usage {
            input_tokens: int(raw, "prompt_tokens").unwrap_or(hit + miss),
            cache_read_tokens: hit,
            output_tokens: int(raw, "completion_tokens").unwrap_or(0),
            reasoning_tokens: nested(raw, "completion_tokens_details", "reasoning_tokens")
                .unwrap_or(0),
            ..Usage::default()
        })
    }
    fn render(&self, usage: &Usage) -> Value {
        let mut out = chat_base(usage);
        out.insert(
            "prompt_cache_hit_tokens".into(),
            json!(usage.cache_read_tokens),
        );
        out.insert(
            "prompt_cache_miss_tokens".into(),
            json!(usage.input_tokens - usage.cache_read_tokens),
        );
        Value::Object(out)
    }
}

/// Baidu Qianfan: `prompt_tokens_details.search_tokens` marks a billed web search.
pub struct QianfanChat;

impl UsageAdapter for QianfanChat {
    fn name(&self) -> &'static str {
        "qianfan_chat"
    }
    fn protocol(&self) -> Protocol {
        Protocol::OpenaiChat
    }
    fn matches(&self, raw: &Map<String, Value>, _protocol: Protocol) -> bool {
        has_nested(raw, "prompt_tokens_details", "search_tokens")
            || has_nested(raw, "prompt_tokens_details", "plugin_tokens")
    }
    fn parse(&self, raw: &Map<String, Value>) -> Option<Usage> {
        let mut usage = chat_parse(raw)?;
        let search = nested(raw, "prompt_tokens_details", "search_tokens").unwrap_or(0);
        usage.search_count = i64::from(search > 0);
        Some(usage)
    }
    fn render(&self, usage: &Usage) -> Value {
        let mut out = chat_base(usage);
        out.insert(
            "prompt_tokens_details".into(),
            json!({
                "cached_tokens": usage.cache_read_tokens,
                "search_tokens": 0,
            }),
        );
        Value::Object(out)
    }
}

/// Moonshot Kimi: top-level `cached_tokens` and/or `prompt_tokens_details.cache_write_tokens`.
pub struct KimiChat;

impl UsageAdapter for KimiChat {
    fn name(&self) -> &'static str {
        "kimi_chat"
    }
    fn protocol(&self) -> Protocol {
        Protocol::OpenaiChat
    }
    fn matches(&self, raw: &Map<String, Value>, _protocol: Protocol) -> bool {
        has(raw, "cached_tokens") || has_nested(raw, "prompt_tokens_details", "cache_write_tokens")
    }
    fn parse(&self, raw: &Map<String, Value>) -> Option<Usage> {
        let mut usage = chat_parse(raw)?;
        if usage.cache_read_tokens == 0 {
            usage.cache_read_tokens = int(raw, "cached_tokens").unwrap_or(0);
        }
        usage.cache_write_tokens =
            nested(raw, "prompt_tokens_details", "cache_write_tokens").unwrap_or(0);
        Some(usage)
    }
    fn render(&self, usage: &Usage) -> Value {
        let mut out = chat_base(usage);
        out.insert("cached_tokens".into(), json!(usage.cache_read_tokens));
        out.insert(
            "prompt_tokens_details".into(),
            json!({
                "cached_tokens": usage.cache_read_tokens,
                "cache_write_tokens": usage.cache_write_tokens,
            }),
        );
        Value::Object(out)
    }
}

/// Embeddings: input only.
pub struct Embeddings;

impl UsageAdapter for Embeddings {
    fn name(&self) -> &'static str {
        "embeddings"
    }
    fn protocol(&self) -> Protocol {
        Protocol::Embeddings
    }
    fn matches(&self, raw: &Map<String, Value>, _protocol: Protocol) -> bool {
        has(raw, "prompt_tokens") && has(raw, "total_tokens") && !has(raw, "completion_tokens")
    }
    fn parse(&self, raw: &Map<String, Value>) -> Option<Usage> {
        Some(Usage {
            input_tokens: int(raw, "prompt_tokens")?,
            ..Usage::default()
        })
    }
    fn render(&self, usage: &Usage) -> Value {
        json!({
            "prompt_tokens": usage.input_tokens,
            "total_tokens": usage.input_tokens,
        })
    }
}

/// OpenAI Chat Completions, the most common field set and the last fallback.
pub struct OpenaiChat;

impl UsageAdapter for OpenaiChat {
    fn name(&self) -> &'static str {
        "openai_chat"
    }
    fn protocol(&self) -> Protocol {
        Protocol::OpenaiChat
    }
    fn matches(&self, raw: &Map<String, Value>, _protocol: Protocol) -> bool {
        has(raw, "prompt_tokens") && has(raw, "completion_tokens")
    }
    fn parse(&self, raw: &Map<String, Value>) -> Option<Usage> {
        chat_parse(raw)
    }
    fn render(&self, usage: &Usage) -> Value {
        Value::Object(chat_base(usage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmatched_usage_fails() {
        let err = match_usage(&json!({"tokens": 3}), Protocol::OpenaiChat).unwrap_err();
        assert!(err.0.contains("no usage adapter"));
        assert!(match_usage(&json!(null), Protocol::OpenaiChat).is_err());
    }

    #[test]
    fn negative_counts_are_malformed() {
        let raw = json!({"prompt_tokens": -1, "completion_tokens": 2});
        assert!(match_usage(&raw, Protocol::OpenaiChat).is_err());
    }

    #[test]
    fn render_then_match_roundtrips() {
        let usage = Usage {
            input_tokens: 1000,
            cache_read_tokens: 600,
            cache_write_tokens: 0,
            output_tokens: 300,
            reasoning_tokens: 120,
            search_count: 0,
            adapter: String::new(),
        };
        for a in ADAPTERS {
            if a.name() == "embeddings" || a.name() == "anthropic_messages" {
                continue;
            }
            let rendered = a.render(&usage);
            let back = match_usage(&rendered, a.protocol()).unwrap();
            assert_eq!(back.adapter, a.name(), "{rendered}");
            assert_eq!(back.input_tokens, 1000, "{}", a.name());
            assert_eq!(back.cache_read_tokens, 600, "{}", a.name());
            assert_eq!(back.output_tokens, 300, "{}", a.name());
            assert_eq!(back.reasoning_tokens, 120, "{}", a.name());
        }
    }
}
