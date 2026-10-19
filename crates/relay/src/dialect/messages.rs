//! Anthropic Messages dialects.

use super::*;
use serde_json::json;

/// [MiniMax Anthropic](https://platform.minimax.io/docs/api-reference/text-anthropic-api):
/// sized by `output_config.effort` with adaptive thinking instead of a token budget.
pub struct Effort;

impl DialectCodec for Effort {
    fn spec(&self) -> &'static Dialect {
        spec(Protocol::AnthropicMessages, "effort")
    }

    fn request_keys(&self) -> &'static [&'static str] {
        &["output_config"]
    }

    fn read_request(&self, body: &Value, req: &mut ir::Request) {
        if let Some(e) = body
            .pointer("/output_config/effort")
            .and_then(Value::as_str)
        {
            let r = req.reasoning.get_or_insert_with(Default::default);
            if !r.is_off() {
                r.effort = Some(e.to_string());
            }
        }
    }

    fn write_request(&self, req: &ir::Request, out: &mut Map<String, Value>) {
        let Some(r) = wants_on(req) else {
            return;
        };
        if r.budget_tokens.is_none() {
            out.insert("thinking".into(), json!({"type": "adaptive"}));
        }
        if r.effort.is_some() {
            out.insert("output_config".into(), json!({"effort": r.effort()}));
        }
    }
}
