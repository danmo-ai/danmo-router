//! OpenAI Responses dialects.

use super::*;
use serde_json::json;

/// [Bailian](https://docs.qwencloud.com/api-reference/chat/openai-responses) and
/// [Ark](https://ark.volcengine.com/region:cn-beijing/docs/responses-api-deep-thinking):
/// `thinking.type` switches thinking, `reasoning.effort` sizes it and must be absent when off.
pub struct Thinking;

impl DialectCodec for Thinking {
    fn spec(&self) -> &'static Dialect {
        spec(Protocol::OpenaiResponses, "thinking")
    }

    fn request_keys(&self) -> &'static [&'static str] {
        &["thinking"]
    }

    fn read_request(&self, body: &Value, req: &mut ir::Request) {
        read_thinking_type(body, req);
    }

    fn write_request(&self, req: &ir::Request, out: &mut Map<String, Value>) {
        if is_off(req) {
            out.remove("reasoning");
            out.insert("thinking".into(), json!({"type": "disabled"}));
        } else if wants_on(req).is_some() {
            out.insert("thinking".into(), json!({"type": "enabled"}));
        }
    }
}
