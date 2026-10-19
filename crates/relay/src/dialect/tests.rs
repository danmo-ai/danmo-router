//! Request and response fixtures in each vendor's documented shape, converted between dialects.

use super::*;
use crate::convert;
use danmo_core::dialect::find;
use danmo_core::usage::Usage;
use serde_json::{json, Value};
use Protocol::*;

fn c(protocol: Protocol, name: &str) -> &'static dyn DialectCodec {
    codec(find(protocol, name).unwrap())
}

fn conv(from: (Protocol, &str), to: (Protocol, &str), body: Value) -> Value {
    convert::request(c(from.0, from.1), c(to.0, to.1), &body, "up", false).unwrap()
}

fn chat(extra: Value) -> Value {
    let mut body = json!({"model": "m", "messages": [{"role": "user", "content": "hi"}]});
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    body
}

const CHAT: Protocol = OpenaiChat;

#[test]
fn thinking_roundtrips_through_standard() {
    let ds = chat(json!({"thinking": {"type": "enabled"}, "reasoning_effort": "high"}));
    let same = conv((CHAT, "thinking"), (CHAT, "thinking"), ds.clone());
    assert_eq!(same["thinking"], json!({"type": "enabled"}));
    assert_eq!(same["reasoning_effort"], "high");

    let std = conv((CHAT, "thinking"), (CHAT, "standard"), ds);
    assert!(std.get("thinking").is_none());
    assert_eq!(std["reasoning_effort"], "high");

    let off = conv(
        (CHAT, "thinking"),
        (CHAT, "standard"),
        chat(json!({"thinking": {"type": "disabled"}})),
    );
    assert_eq!(off["reasoning_effort"], "none");
    let back = conv((CHAT, "standard"), (CHAT, "thinking"), off);
    assert_eq!(back["thinking"], json!({"type": "disabled"}));
    assert!(back.get("reasoning_effort").is_none());
}

#[test]
fn enable_thinking_maps_switch_and_budget() {
    let bailian = chat(json!({"enable_thinking": true, "thinking_budget": 4096}));
    let same = conv(
        (CHAT, "enable_thinking"),
        (CHAT, "enable_thinking"),
        bailian.clone(),
    );
    assert_eq!(same["enable_thinking"], true);
    assert_eq!(same["thinking_budget"], 4096);
    assert!(same.get("reasoning_effort").is_none());

    let ds = conv((CHAT, "enable_thinking"), (CHAT, "thinking"), bailian);
    assert_eq!(ds["thinking"], json!({"type": "enabled"}));
    assert_eq!(ds["reasoning_effort"], "low");

    let off = conv(
        (CHAT, "thinking"),
        (CHAT, "enable_thinking"),
        chat(json!({"thinking": {"type": "disabled"}})),
    );
    assert_eq!(off["enable_thinking"], false);
    assert!(off.get("thinking").is_none());
    assert!(off.get("reasoning_effort").is_none());

    let unset = conv(
        (CHAT, "thinking"),
        (CHAT, "enable_thinking"),
        chat(json!({})),
    );
    assert!(unset.get("enable_thinking").is_none());
}

#[test]
fn kimi_and_qianfan_use_their_own_sizing() {
    let kimi =
        chat(json!({"thinking": {"type": "enabled", "keep": "all"}, "reasoning_effort": "medium"}));
    let same = conv((CHAT, "kimi"), (CHAT, "kimi"), kimi.clone());
    assert_eq!(same["thinking"]["type"], "enabled");
    assert_eq!(same["reasoning_effort"], "medium");

    let qf = conv((CHAT, "kimi"), (CHAT, "qianfan"), kimi);
    assert_eq!(qf["thinking"], json!({"type": "enabled"}));
    assert_eq!(qf["thinking_budget"], 8192);
    assert!(qf.get("reasoning_effort").is_none());

    let qianfan = chat(
        json!({"thinking": {"type": "enabled"}, "thinking_budget": 2048,
        "web_search": {"enable": true}}),
    );
    let bailian = conv(
        (CHAT, "qianfan"),
        (CHAT, "enable_thinking"),
        qianfan.clone(),
    );
    assert_eq!(bailian["enable_thinking"], true);
    assert_eq!(bailian["thinking_budget"], 2048);
    assert_eq!(bailian["web_search"], json!({"enable": true}));
    let same = conv((CHAT, "qianfan"), (CHAT, "qianfan"), qianfan);
    assert_eq!(same["thinking_budget"], 2048);
}

#[test]
fn qianfan_ernie_owns_its_strategy_field() {
    let ernie = chat(json!({"enable_thinking": true, "thinking_strategy": "short_think"}));
    let same = conv(
        (CHAT, "qianfan_ernie"),
        (CHAT, "qianfan_ernie"),
        ernie.clone(),
    );
    assert_eq!(same["enable_thinking"], true);
    let qf = conv((CHAT, "qianfan_ernie"), (CHAT, "qianfan"), ernie);
    assert_eq!(qf["thinking"], json!({"type": "enabled"}));
    assert!(qf.get("thinking_strategy").is_none());
    assert!(qf.get("enable_thinking").is_none());
}

#[test]
fn hunyuan_has_no_thinking_switch() {
    let ds = chat(json!({"thinking": {"type": "enabled"}, "reasoning_effort": "high"}));
    let hy = conv((CHAT, "thinking"), (CHAT, "hunyuan"), ds);
    assert!(hy.get("reasoning_effort").is_none());
    assert!(hy.get("thinking").is_none());
    let own = chat(json!({"enable_enhancement": true, "search_info": true, "citation": true}));
    let std = conv((CHAT, "hunyuan"), (CHAT, "standard"), own);
    assert!(std.get("enable_enhancement").is_none());
    assert!(std.get("citation").is_none());
}

#[test]
fn minimax_is_adaptive_without_none_and_splits_reasoning() {
    let on = conv(
        (CHAT, "standard"),
        (CHAT, "minimax"),
        chat(json!({"reasoning_effort": "minimal"})),
    );
    assert_eq!(on["thinking"], json!({"type": "adaptive"}));
    assert_eq!(on["reasoning_effort"], "low");
    assert_eq!(on["reasoning_split"], true);

    let off = conv(
        (CHAT, "standard"),
        (CHAT, "minimax"),
        chat(json!({"reasoning_effort": "none"})),
    );
    assert_eq!(off["thinking"], json!({"type": "disabled"}));
    assert!(off.get("reasoning_effort").is_none());

    let back = conv(
        (CHAT, "minimax"),
        (CHAT, "thinking"),
        chat(json!({"thinking": {"type": "adaptive"}, "reasoning_split": true})),
    );
    assert_eq!(back["thinking"], json!({"type": "enabled"}));
    assert!(back.get("reasoning_split").is_none());
}

#[test]
fn responses_thinking_drops_reasoning_when_off() {
    let body = json!({"model": "m", "input": "hi", "thinking": {"type": "disabled"},
        "reasoning": {"effort": "high"}});
    let out = conv(
        (OpenaiResponses, "thinking"),
        (OpenaiResponses, "thinking"),
        body,
    );
    assert_eq!(out["thinking"], json!({"type": "disabled"}));
    assert!(out.get("reasoning").is_none());

    let std = json!({"model": "m", "input": "hi", "reasoning": {"effort": "high"}});
    let ark = conv(
        (OpenaiResponses, "standard"),
        (OpenaiResponses, "thinking"),
        std,
    );
    assert_eq!(ark["thinking"], json!({"type": "enabled"}));
    assert_eq!(ark["reasoning"]["effort"], "high");
}

#[test]
fn anthropic_effort_uses_adaptive_thinking() {
    let out = conv(
        (CHAT, "thinking"),
        (AnthropicMessages, "effort"),
        chat(json!({"thinking": {"type": "enabled"}, "reasoning_effort": "high"})),
    );
    assert_eq!(out["thinking"], json!({"type": "adaptive"}));
    assert_eq!(out["output_config"], json!({"effort": "high"}));

    let budget = json!({"model": "m", "max_tokens": 64, "messages": [{"role": "user", "content": "hi"}],
        "thinking": {"type": "enabled", "budget_tokens": 2048}});
    let kept = conv(
        (AnthropicMessages, "standard"),
        (AnthropicMessages, "effort"),
        budget,
    );
    assert_eq!(kept["thinking"]["type"], "enabled");
    assert_eq!(kept["thinking"]["budget_tokens"], 2048);

    let mm = json!({"model": "m", "max_tokens": 64, "messages": [{"role": "user", "content": "hi"}],
        "thinking": {"type": "adaptive"}, "output_config": {"effort": "high"}});
    let std = conv(
        (AnthropicMessages, "effort"),
        (AnthropicMessages, "standard"),
        mm,
    );
    assert_eq!(std["thinking"]["type"], "enabled");
    assert_eq!(std["thinking"]["budget_tokens"], 24576);
    assert!(std.get("output_config").is_none());

    let off = conv(
        (CHAT, "thinking"),
        (AnthropicMessages, "effort"),
        chat(json!({"thinking": {"type": "disabled"}})),
    );
    assert_eq!(off["thinking"], json!({"type": "disabled"}));
}

#[test]
fn unknown_fields_survive_within_a_protocol_only() {
    let body = json!({
        "model": "m",
        "zz_vendor": {"a": 1},
        "messages": [
            {"role": "user", "content": "hi", "name": "bob"},
            {"role": "assistant", "content": "", "reasoning_content": "", "prefix": true}
        ]
    });
    let qf = conv((CHAT, "thinking"), (CHAT, "qianfan"), body.clone());
    assert_eq!(qf["zz_vendor"], json!({"a": 1}));
    assert_eq!(qf["messages"][0]["name"], "bob");
    assert_eq!(qf["messages"][1]["prefix"], true);
    assert_eq!(qf["messages"][1]["reasoning_content"], "");

    let msgs = conv((CHAT, "thinking"), (AnthropicMessages, "standard"), body);
    assert!(msgs.get("zz_vendor").is_none());
    assert!(msgs["messages"][0].get("name").is_none());
}

fn usage() -> Usage {
    Usage {
        input_tokens: 10,
        output_tokens: 5,
        adapter: "openai_chat".into(),
        ..Usage::default()
    }
}

fn chat_turn(extra: Value) -> Value {
    let mut body = json!({
        "model": "m",
        "max_tokens": 64,
        "messages": [
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": "a", "reasoning_content": "plan"},
            {"role": "user", "content": "go"}
        ]
    });
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    body
}

#[test]
fn reasoning_switch_converts_across_protocols() {
    let on = chat_turn(json!({"thinking": {"type": "enabled"}, "reasoning_effort": "high"}));

    let msgs = conv(
        (CHAT, "thinking"),
        (AnthropicMessages, "standard"),
        on.clone(),
    );
    assert_eq!(msgs["thinking"]["type"], "enabled");
    assert_eq!(msgs["thinking"]["budget_tokens"], 24576);
    assert!(msgs["max_tokens"].as_i64().unwrap() > 24576);
    assert!(msgs.get("reasoning_effort").is_none());
    assert!(msgs.get("thinking").unwrap().get("keep").is_none());
    // Unsigned chat reasoning cannot be replayed as an Anthropic thinking block.
    assert_eq!(
        msgs["messages"][1]["content"],
        json!([{"type": "text", "text": "a"}])
    );

    let gemini = conv((CHAT, "thinking"), (Gemini, "standard"), on.clone());
    assert_eq!(
        gemini["generationConfig"]["thinkingConfig"],
        json!({"thinkingBudget": 24576, "includeThoughts": true})
    );
    assert_eq!(
        gemini["contents"][1]["parts"][0],
        json!({"text": "plan", "thought": true})
    );

    let responses = conv(
        (CHAT, "enable_thinking"),
        (OpenaiResponses, "thinking"),
        chat(json!({"enable_thinking": true, "thinking_budget": 4096})),
    );
    assert_eq!(responses["thinking"], json!({"type": "enabled"}));
    assert_eq!(responses["reasoning"]["effort"], "low");

    let off = chat_turn(json!({"thinking": {"type": "disabled"}}));
    let msgs_off = conv(
        (CHAT, "thinking"),
        (AnthropicMessages, "standard"),
        off.clone(),
    );
    assert_eq!(msgs_off["thinking"], json!({"type": "disabled"}));
    let gemini_off = conv((CHAT, "thinking"), (Gemini, "standard"), off);
    assert_eq!(
        gemini_off["generationConfig"]["thinkingConfig"],
        json!({"thinkingBudget": 0})
    );
}

#[test]
fn each_protocol_reads_its_reasoning_into_the_target_dialect() {
    let ark = json!({"model": "m", "input": "hi", "thinking": {"type": "disabled"},
        "reasoning": {"effort": "high"}});
    let chat_off = conv((OpenaiResponses, "thinking"), (CHAT, "standard"), ark);
    assert_eq!(chat_off["reasoning_effort"], "none");
    assert!(chat_off.get("thinking").is_none());

    let gemini = json!({
        "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
        "generationConfig": {"thinkingConfig": {"thinkingLevel": "HIGH"}}
    });
    let qf = conv((Gemini, "standard"), (CHAT, "qianfan"), gemini);
    assert_eq!(qf["thinking"], json!({"type": "enabled"}));
    assert_eq!(qf["thinking_budget"], 24576);
    assert!(qf.get("reasoning_effort").is_none());

    let mm = json!({
        "model": "m", "max_tokens": 100,
        "messages": [{"role": "user", "content": "hi"}],
        "thinking": {"type": "adaptive"},
        "output_config": {"effort": "high"}
    });
    let minimax = conv((AnthropicMessages, "effort"), (CHAT, "minimax"), mm);
    assert_eq!(minimax["thinking"], json!({"type": "adaptive"}));
    assert_eq!(minimax["reasoning_effort"], "high");
    assert_eq!(minimax["reasoning_split"], true);
    assert!(minimax.get("output_config").is_none());
}

#[test]
fn dialect_only_fields_do_not_cross_protocols() {
    let body = chat(json!({
        "thinking": {"type": "enabled"},
        "web_search": {"enable": true},
        "enable_enhancement": true,
        "zz_client": 7
    }));
    let msgs = conv((CHAT, "qianfan"), (AnthropicMessages, "effort"), body);
    assert!(msgs.get("web_search").is_none());
    assert!(msgs.get("enable_enhancement").is_none());
    assert!(msgs.get("zz_client").is_none());
    assert_eq!(msgs["thinking"]["type"], "adaptive");
}

#[test]
fn response_reasoning_converts_across_protocols() {
    let chat_resp = json!({
        "id": "c", "model": "m",
        "choices": [{"index": 0, "finish_reason": "stop", "message": {
            "role": "assistant", "content": "ok", "reasoning_content": "hmm"
        }}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    });
    let gemini = convert::response(
        c(CHAT, "thinking"),
        c(Gemini, "standard"),
        &chat_resp,
        &usage(),
        "m",
    )
    .unwrap();
    assert_eq!(
        gemini["candidates"][0]["content"]["parts"],
        json!([{"text": "hmm", "thought": true}, {"text": "ok"}])
    );
    assert_eq!(gemini["usageMetadata"]["promptTokenCount"], 10);
    assert_eq!(gemini["usageMetadata"]["candidatesTokenCount"], 5);

    let msgs = json!({
        "id": "m1", "type": "message", "role": "assistant", "model": "m",
        "content": [
            {"type": "thinking", "thinking": "hmm", "signature": "sig"},
            {"type": "text", "text": "ok"}
        ],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 10, "output_tokens": 5}
    });
    let back = convert::response(
        c(AnthropicMessages, "standard"),
        c(CHAT, "thinking"),
        &msgs,
        &usage(),
        "m",
    )
    .unwrap();
    assert_eq!(back["choices"][0]["message"]["reasoning_content"], "hmm");
    assert_eq!(back["choices"][0]["message"]["content"], "ok");
    assert_eq!(back["usage"]["prompt_cache_hit_tokens"], 0);
    assert_eq!(back["usage"]["prompt_cache_miss_tokens"], 10);
}

#[test]
fn converted_stream_renders_usage_in_the_client_dialect() {
    use crate::sse::SseParser;

    let mut conv = convert::StreamConverter::new(
        c(AnthropicMessages, "standard"),
        c(CHAT, "thinking"),
        "client".into(),
        true,
    );
    let raw = "\
event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"m1\",\"usage\":{\"input_tokens\":60,\"cache_read_input_tokens\":40,\"output_tokens\":1}}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"hmm\"}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\n";
    let mut parser = SseParser::default();
    let mut out = Vec::new();
    for ev in parser.push(raw.as_bytes()) {
        let json = ev.json();
        out.extend(conv.event(&ev, json.as_ref()));
    }
    out.extend(conv.finish());
    let text: String = out
        .iter()
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .collect();
    assert!(text.contains("\"reasoning_content\":\"hmm\""), "{text}");
    assert!(text.contains("\"content\":\"ok\""), "{text}");
    assert!(text.contains("\"prompt_cache_hit_tokens\":40"), "{text}");
    assert!(text.contains("\"prompt_cache_miss_tokens\":60"), "{text}");
}

#[test]
fn responses_keep_extras_and_read_reasoning_details() {
    let mm = json!({
        "id": "x", "object": "chat.completion", "model": "MiniMax-M2",
        "choices": [{"index": 0, "finish_reason": "stop", "message": {
            "role": "assistant", "content": "ok",
            "reasoning_details": [{"type": "reasoning.text", "text": "think"}]
        }}],
        "base_resp": {"status_code": 0, "status_msg": ""},
        "search_results": [{"url": "u"}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    });
    let out =
        convert::response(c(CHAT, "minimax"), c(CHAT, "thinking"), &mm, &usage(), "m").unwrap();
    assert_eq!(out["choices"][0]["message"]["reasoning_content"], "think");
    assert_eq!(out["search_results"], json!([{"url": "u"}]));
    assert!(out.get("base_resp").is_none());
    assert_eq!(out["usage"]["prompt_cache_hit_tokens"], 0);

    let msgs = convert::response(
        c(CHAT, "minimax"),
        c(AnthropicMessages, "standard"),
        &mm,
        &usage(),
        "m",
    )
    .unwrap();
    assert!(msgs.get("search_results").is_none());
    assert_eq!(msgs["usage"]["input_tokens"], 10);
}
