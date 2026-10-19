//! Non-streaming usage objects as returned by vendors and third-party relays.
//! Each case asserts the adapter chosen by field features and the standard numbers.

use danmo_core::protocol::Protocol;
use danmo_core::usage::{match_usage, Usage};
use serde_json::json;

struct Expect {
    adapter: &'static str,
    input: i64,
    cache_read: i64,
    cache_write: i64,
    output: i64,
    reasoning: i64,
    search: i64,
}

fn check(name: &str, raw: serde_json::Value, protocol: Protocol, e: Expect) {
    let u: Usage = match_usage(&raw, protocol).unwrap_or_else(|err| panic!("{name}: {err}"));
    assert_eq!(u.adapter, e.adapter, "{name} adapter");
    assert_eq!(u.input_tokens, e.input, "{name} input");
    assert_eq!(u.cache_read_tokens, e.cache_read, "{name} cache_read");
    assert_eq!(u.cache_write_tokens, e.cache_write, "{name} cache_write");
    assert_eq!(u.output_tokens, e.output, "{name} output");
    assert_eq!(u.reasoning_tokens, e.reasoning, "{name} reasoning");
    assert_eq!(u.search_count, e.search, "{name} search");
}

#[test]
fn deepseek_chat_official() {
    check(
        "deepseek",
        json!({
            "completion_tokens": 120,
            "prompt_tokens": 1000,
            "prompt_cache_hit_tokens": 640,
            "prompt_cache_miss_tokens": 360,
            "total_tokens": 1120,
            "prompt_tokens_details": {"cached_tokens": 640},
            "completion_tokens_details": {"reasoning_tokens": 80}
        }),
        Protocol::OpenaiChat,
        Expect {
            adapter: "deepseek_chat",
            input: 1000,
            cache_read: 640,
            cache_write: 0,
            output: 120,
            reasoning: 80,
            search: 0,
        },
    );
}

#[test]
fn deepseek_anthropic_endpoint() {
    check(
        "deepseek /anthropic",
        json!({"input_tokens": 360, "cache_read_input_tokens": 640, "output_tokens": 120}),
        Protocol::AnthropicMessages,
        Expect {
            adapter: "anthropic_messages",
            input: 1000,
            cache_read: 640,
            cache_write: 0,
            output: 120,
            reasoning: 0,
            search: 0,
        },
    );
}

#[test]
fn kimi_chat_official() {
    check(
        "kimi",
        json!({
            "prompt_tokens": 2000,
            "completion_tokens": 50,
            "total_tokens": 2050,
            "cached_tokens": 1536,
            "prompt_tokens_details": {"cached_tokens": 1536}
        }),
        Protocol::OpenaiChat,
        Expect {
            adapter: "kimi_chat",
            input: 2000,
            cache_read: 1536,
            cache_write: 0,
            output: 50,
            reasoning: 0,
            search: 0,
        },
    );
}

#[test]
fn kimi_chat_cache_write() {
    check(
        "kimi cache write",
        json!({
            "prompt_tokens": 3000,
            "completion_tokens": 10,
            "total_tokens": 3010,
            "prompt_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 2048}
        }),
        Protocol::OpenaiChat,
        Expect {
            adapter: "kimi_chat",
            input: 3000,
            cache_read: 0,
            cache_write: 2048,
            output: 10,
            reasoning: 0,
            search: 0,
        },
    );
}

#[test]
fn qwen_compatible_mode() {
    check(
        "qwen",
        json!({
            "prompt_tokens": 3019,
            "completion_tokens": 104,
            "total_tokens": 3123,
            "prompt_tokens_details": {"cached_tokens": 2048}
        }),
        Protocol::OpenaiChat,
        Expect {
            adapter: "openai_chat",
            input: 3019,
            cache_read: 2048,
            cache_write: 0,
            output: 104,
            reasoning: 0,
            search: 0,
        },
    );
}

#[test]
fn qwen_responses() {
    check(
        "qwen responses",
        json!({
            "input_tokens": 54,
            "input_tokens_details": {"cached_tokens": 0},
            "output_tokens": 300,
            "output_tokens_details": {"reasoning_tokens": 256},
            "total_tokens": 354
        }),
        Protocol::OpenaiResponses,
        Expect {
            adapter: "openai_responses",
            input: 54,
            cache_read: 0,
            cache_write: 0,
            output: 300,
            reasoning: 256,
            search: 0,
        },
    );
}

#[test]
fn glm_chat() {
    check(
        "glm chat",
        json!({
            "prompt_tokens": 500,
            "completion_tokens": 900,
            "total_tokens": 1400,
            "prompt_tokens_details": {"cached_tokens": 128},
            "completion_tokens_details": {"reasoning_tokens": 700}
        }),
        Protocol::OpenaiChat,
        Expect {
            adapter: "openai_chat",
            input: 500,
            cache_read: 128,
            cache_write: 0,
            output: 900,
            reasoning: 700,
            search: 0,
        },
    );
}

#[test]
fn glm_anthropic_endpoint_with_chat_details() {
    check(
        "glm /api/anthropic",
        json!({
            "input_tokens": 372,
            "output_tokens": 60,
            "cache_read_input_tokens": 128,
            "prompt_tokens_details": {"cached_tokens": 128}
        }),
        Protocol::AnthropicMessages,
        Expect {
            adapter: "anthropic_messages",
            input: 500,
            cache_read: 128,
            cache_write: 0,
            output: 60,
            reasoning: 0,
            search: 0,
        },
    );
}

#[test]
fn doubao_ark_chat() {
    check(
        "ark",
        json!({
            "completion_tokens": 1179,
            "prompt_tokens": 20,
            "total_tokens": 1199,
            "prompt_tokens_details": {"cached_tokens": 0},
            "completion_tokens_details": {"reasoning_tokens": 1036}
        }),
        Protocol::OpenaiChat,
        Expect {
            adapter: "openai_chat",
            input: 20,
            cache_read: 0,
            cache_write: 0,
            output: 1179,
            reasoning: 1036,
            search: 0,
        },
    );
}

#[test]
fn minimax_chat_and_anthropic() {
    check(
        "minimax chat",
        json!({
            "total_tokens": 250,
            "prompt_tokens": 50,
            "completion_tokens": 200,
            "completion_tokens_details": {"reasoning_tokens": 150}
        }),
        Protocol::OpenaiChat,
        Expect {
            adapter: "openai_chat",
            input: 50,
            cache_read: 0,
            cache_write: 0,
            output: 200,
            reasoning: 150,
            search: 0,
        },
    );
    check(
        "minimax /anthropic",
        json!({"input_tokens": 50, "output_tokens": 200}),
        Protocol::AnthropicMessages,
        Expect {
            adapter: "anthropic_messages",
            input: 50,
            cache_read: 0,
            cache_write: 0,
            output: 200,
            reasoning: 0,
            search: 0,
        },
    );
}

#[test]
fn qianfan_with_search() {
    check(
        "qianfan",
        json!({
            "prompt_tokens": 1500,
            "completion_tokens": 300,
            "total_tokens": 1800,
            "prompt_tokens_details": {"search_tokens": 1200}
        }),
        Protocol::OpenaiChat,
        Expect {
            adapter: "qianfan_chat",
            input: 1500,
            cache_read: 0,
            cache_write: 0,
            output: 300,
            reasoning: 0,
            search: 1,
        },
    );
}

#[test]
fn siliconflow_serving_deepseek() {
    check(
        "siliconflow",
        json!({
            "prompt_tokens": 1000,
            "completion_tokens": 120,
            "total_tokens": 1120,
            "completion_tokens_details": {"reasoning_tokens": 80}
        }),
        Protocol::OpenaiChat,
        Expect {
            adapter: "openai_chat",
            input: 1000,
            cache_read: 0,
            cache_write: 0,
            output: 120,
            reasoning: 80,
            search: 0,
        },
    );
}

#[test]
fn openrouter_with_cost() {
    check(
        "openrouter",
        json!({
            "prompt_tokens": 194,
            "completion_tokens": 2,
            "total_tokens": 196,
            "cost": 0.95,
            "is_byok": false,
            "prompt_tokens_details": {"cached_tokens": 0},
            "completion_tokens_details": {"reasoning_tokens": 0}
        }),
        Protocol::OpenaiChat,
        Expect {
            adapter: "openai_chat",
            input: 194,
            cache_read: 0,
            cache_write: 0,
            output: 2,
            reasoning: 0,
            search: 0,
        },
    );
}

#[test]
fn anthropic_with_cache_write_and_search() {
    check(
        "anthropic",
        json!({
            "input_tokens": 10,
            "cache_creation_input_tokens": 2000,
            "cache_read_input_tokens": 0,
            "output_tokens": 300,
            "server_tool_use": {"web_search_requests": 2}
        }),
        Protocol::AnthropicMessages,
        Expect {
            adapter: "anthropic_messages",
            input: 2010,
            cache_read: 0,
            cache_write: 2000,
            output: 300,
            reasoning: 0,
            search: 2,
        },
    );
}

#[test]
fn gemini_usage_metadata() {
    check(
        "gemini",
        json!({
            "promptTokenCount": 100,
            "candidatesTokenCount": 40,
            "thoughtsTokenCount": 60,
            "cachedContentTokenCount": 20,
            "totalTokenCount": 200
        }),
        Protocol::Gemini,
        Expect {
            adapter: "gemini",
            input: 100,
            cache_read: 20,
            cache_write: 0,
            output: 100,
            reasoning: 60,
            search: 0,
        },
    );
}

#[test]
fn embeddings_usage() {
    check(
        "embeddings",
        json!({"prompt_tokens": 8, "total_tokens": 8}),
        Protocol::Embeddings,
        Expect {
            adapter: "embeddings",
            input: 8,
            cache_read: 0,
            cache_write: 0,
            output: 0,
            reasoning: 0,
            search: 0,
        },
    );
}

#[test]
fn unknown_field_set_is_rejected() {
    let raw = json!({"total_tokens": 200});
    assert!(match_usage(&raw, Protocol::OpenaiChat).is_err());
}
