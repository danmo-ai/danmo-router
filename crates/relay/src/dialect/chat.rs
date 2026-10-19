//! OpenAI Chat dialects of Chinese vendors. Each one only differs from the base Chat codec in
//! how thinking is switched and sized, plus its own extension fields.

use super::*;
use serde_json::json;

const EFFORT: &str = "reasoning_effort";

fn chat(name: &str) -> &'static Dialect {
    spec(Protocol::OpenaiChat, name)
}

/// `thinking: {type: enabled|disabled}` with `reasoning_effort` sizing.
fn write_thinking_type(req: &ir::Request, out: &mut Map<String, Value>, on: &str) {
    if is_off(req) {
        out.remove(EFFORT);
        out.insert("thinking".into(), json!({"type": "disabled"}));
    } else if wants_on(req).is_some() {
        out.insert("thinking".into(), json!({"type": on}));
    }
}

/// `{flag}: bool` with `thinking_budget` sizing.
fn write_enable_flag(req: &ir::Request, out: &mut Map<String, Value>, flag: &str) {
    out.remove(EFFORT);
    if is_off(req) {
        out.insert(flag.into(), json!(false));
    } else if let Some(r) = wants_on(req) {
        out.insert(flag.into(), json!(true));
        if r.has_level() {
            out.insert("thinking_budget".into(), json!(r.budget()));
        }
    }
}

/// [DeepSeek](https://api-docs.deepseek.com/guides/thinking_mode/),
/// [Ark](https://ark.volcengine.com/region:cn-beijing/docs/compatible-with-openai-sdk) and
/// [GLM](https://docs.bigmodel.cn/cn/guide/capabilities/thinking).
pub struct Thinking;

impl DialectCodec for Thinking {
    fn spec(&self) -> &'static Dialect {
        chat("thinking")
    }

    fn request_keys(&self) -> &'static [&'static str] {
        &["thinking"]
    }

    fn read_request(&self, body: &Value, req: &mut ir::Request) {
        read_thinking_type(body, req);
    }

    fn write_request(&self, req: &ir::Request, out: &mut Map<String, Value>) {
        write_thinking_type(req, out, "enabled");
    }
}

/// [Bailian](https://help.aliyun.com/zh/model-studio/deep-thinking): `enable_thinking` and
/// `thinking_budget` for every model, including DeepSeek and Kimi hosted there.
pub struct EnableThinking;

impl DialectCodec for EnableThinking {
    fn spec(&self) -> &'static Dialect {
        chat("enable_thinking")
    }

    fn request_keys(&self) -> &'static [&'static str] {
        &["enable_thinking", "thinking_budget"]
    }

    fn read_request(&self, body: &Value, req: &mut ir::Request) {
        read_enable_flag(body, "enable_thinking", req);
        read_budget(body, "thinking_budget", req);
    }

    fn write_request(&self, req: &ir::Request, out: &mut Map<String, Value>) {
        write_enable_flag(req, out, "enable_thinking");
    }
}

/// [Kimi](https://platform.kimi.ai/docs/api/chat): `thinking.type` (with Kimi-only `keep`)
/// and `reasoning_effort`.
pub struct Kimi;

impl DialectCodec for Kimi {
    fn spec(&self) -> &'static Dialect {
        chat("kimi")
    }

    fn request_keys(&self) -> &'static [&'static str] {
        &["thinking"]
    }

    fn read_request(&self, body: &Value, req: &mut ir::Request) {
        read_thinking_type(body, req);
    }

    fn write_request(&self, req: &ir::Request, out: &mut Map<String, Value>) {
        write_thinking_type(req, out, "enabled");
    }
}

/// [Qianfan](https://cloud.baidu.com/doc/qianfan-docs/s/Wm95lyynv) for DeepSeek, GLM and
/// Kimi: `thinking.type` sized by `thinking_budget`; `web_search` is passed as is.
pub struct Qianfan;

impl DialectCodec for Qianfan {
    fn spec(&self) -> &'static Dialect {
        chat("qianfan")
    }

    fn request_keys(&self) -> &'static [&'static str] {
        &["thinking", "thinking_budget"]
    }

    fn read_request(&self, body: &Value, req: &mut ir::Request) {
        read_thinking_type(body, req);
        read_budget(body, "thinking_budget", req);
    }

    fn write_request(&self, req: &ir::Request, out: &mut Map<String, Value>) {
        out.remove(EFFORT);
        write_thinking_type(req, out, "enabled");
        if let Some(r) = wants_on(req).filter(|r| r.has_level()) {
            out.insert("thinking_budget".into(), json!(r.budget()));
        }
    }
}

/// Qianfan for ERNIE and Qwen3: `enable_thinking` sized by `thinking_budget`;
/// `thinking_strategy` has no counterpart elsewhere.
pub struct QianfanErnie;

impl DialectCodec for QianfanErnie {
    fn spec(&self) -> &'static Dialect {
        chat("qianfan_ernie")
    }

    fn request_keys(&self) -> &'static [&'static str] {
        &["enable_thinking", "thinking_budget", "thinking_strategy"]
    }

    fn read_request(&self, body: &Value, req: &mut ir::Request) {
        read_enable_flag(body, "enable_thinking", req);
        read_budget(body, "thinking_budget", req);
    }

    fn write_request(&self, req: &ir::Request, out: &mut Map<String, Value>) {
        write_enable_flag(req, out, "enable_thinking");
    }
}

/// [Hunyuan](https://cloud.tencent.com/document/product/1729/111007): no thinking switch;
/// search extensions only exist here.
pub struct Hunyuan;

impl DialectCodec for Hunyuan {
    fn spec(&self) -> &'static Dialect {
        chat("hunyuan")
    }

    fn request_keys(&self) -> &'static [&'static str] {
        &["enable_enhancement", "search_info", "citation"]
    }

    fn write_request(&self, _req: &ir::Request, out: &mut Map<String, Value>) {
        out.remove(EFFORT);
    }
}

/// [MiniMax](https://platform.minimax.io/docs/api-reference/text-openai-api): thinking is
/// `adaptive` or `disabled`, `reasoning_effort` has no `none`, and `reasoning_split` moves
/// reasoning out of `<think>` tags into `reasoning_details`. Failures arrive in `base_resp`.
pub struct Minimax;

impl DialectCodec for Minimax {
    fn spec(&self) -> &'static Dialect {
        chat("minimax")
    }

    fn request_keys(&self) -> &'static [&'static str] {
        &["thinking", "reasoning_split"]
    }

    fn response_keys(&self) -> &'static [&'static str] {
        &["base_resp", "input_sensitive", "output_sensitive"]
    }

    fn read_request(&self, body: &Value, req: &mut ir::Request) {
        read_thinking_type(body, req);
    }

    fn write_request(&self, req: &ir::Request, out: &mut Map<String, Value>) {
        write_thinking_type(req, out, "adaptive");
        if let Some(e) = out.get(EFFORT).and_then(Value::as_str) {
            let mapped = match e {
                "minimal" => "low",
                other => other,
            };
            out.insert(EFFORT.into(), json!(mapped));
        }
        out.insert("reasoning_split".into(), json!(true));
    }

    fn soft_error(&self, body: &Value) -> Option<String> {
        base_resp_error(body)
    }
}
