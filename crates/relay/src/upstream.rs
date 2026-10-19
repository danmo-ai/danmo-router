use axum::http::{HeaderMap, HeaderName, HeaderValue};
use danmo_core::protocol::{AuthStyle, Protocol};

/// Headers that never cross the proxy: hop-by-hop, client credentials, and encodings we would
/// have to decode to read usage.
const DROP_REQUEST: &[&str] = &[
    "host",
    "content-length",
    "content-type",
    "authorization",
    "x-api-key",
    "x-goog-api-key",
    "api-key",
    "cookie",
    "connection",
    "keep-alive",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "accept-encoding",
];

const DROP_RESPONSE: &[&str] = &[
    "content-length",
    "content-encoding",
    "transfer-encoding",
    "connection",
    "keep-alive",
    "set-cookie",
    "trailer",
    "upgrade",
];

fn has_path(base: &str) -> bool {
    let rest = base.split_once("://").map_or(base, |(_, r)| r);
    rest.split_once('/').is_some_and(|(_, p)| !p.is_empty())
}

pub fn url(base_url: &str, protocol: Protocol, upstream_model: &str, stream: bool) -> String {
    let base = base_url.trim_end_matches('/');
    match protocol {
        Protocol::OpenaiChat | Protocol::OpenaiResponses | Protocol::Embeddings => {
            let prefix = if has_path(base) {
                base.to_string()
            } else {
                format!("{base}/v1")
            };
            let tail = match protocol {
                Protocol::OpenaiChat => "chat/completions",
                Protocol::OpenaiResponses => "responses",
                _ => "embeddings",
            };
            format!("{prefix}/{tail}")
        }
        Protocol::AnthropicMessages => {
            if base.ends_with("/v1") {
                format!("{base}/messages")
            } else {
                format!("{base}/v1/messages")
            }
        }
        Protocol::Gemini => {
            let prefix = if has_path(base) {
                base.to_string()
            } else {
                format!("{base}/v1beta")
            };
            if stream {
                format!("{prefix}/models/{upstream_model}:streamGenerateContent?alt=sse")
            } else {
                format!("{prefix}/models/{upstream_model}:generateContent")
            }
        }
    }
}

pub fn request_headers(
    client: &HeaderMap,
    protocol: Protocol,
    auth: AuthStyle,
    key: &str,
) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (name, value) in client {
        if !DROP_REQUEST.contains(&name.as_str()) {
            out.append(name.clone(), value.clone());
        }
    }
    out.insert("content-type", HeaderValue::from_static("application/json"));
    if protocol == Protocol::AnthropicMessages && !out.contains_key("anthropic-version") {
        out.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    }
    let (name, value) = match auth {
        AuthStyle::Bearer => ("authorization", format!("Bearer {key}")),
        AuthStyle::XApiKey => ("x-api-key", key.to_string()),
        AuthStyle::XGoogApiKey => ("x-goog-api-key", key.to_string()),
    };
    if let Ok(v) = HeaderValue::from_str(&value) {
        out.insert(HeaderName::from_static(name), v);
    }
    out
}

/// Passthrough keeps upstream response headers (request ids, rate-limit info) minus hop-by-hop.
pub fn response_headers(upstream: &reqwest::header::HeaderMap) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (name, value) in upstream {
        if DROP_RESPONSE.contains(&name.as_str()) {
            continue;
        }
        if let (Ok(n), Ok(v)) = (
            HeaderName::from_bytes(name.as_str().as_bytes()),
            HeaderValue::from_bytes(value.as_bytes()),
        ) {
            out.append(n, v);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_urls_per_protocol() {
        let chat = Protocol::OpenaiChat;
        assert_eq!(
            url("https://api.openai.com", chat, "m", false),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            url("https://api.moonshot.cn/v1/", chat, "m", false),
            "https://api.moonshot.cn/v1/chat/completions"
        );
        assert_eq!(
            url("https://open.bigmodel.cn/api/paas/v4", chat, "m", false),
            "https://open.bigmodel.cn/api/paas/v4/chat/completions"
        );
        assert_eq!(
            url(
                "https://dashscope.aliyuncs.com/compatible-mode/v1",
                Protocol::OpenaiResponses,
                "m",
                true
            ),
            "https://dashscope.aliyuncs.com/compatible-mode/v1/responses"
        );
        assert_eq!(
            url(
                "https://api.deepseek.com/anthropic",
                Protocol::AnthropicMessages,
                "m",
                true
            ),
            "https://api.deepseek.com/anthropic/v1/messages"
        );
        assert_eq!(
            url(
                "https://generativelanguage.googleapis.com",
                Protocol::Gemini,
                "gemini-2.5-pro",
                true
            ),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn replaces_auth_and_drops_hop_by_hop() {
        let mut client = HeaderMap::new();
        client.insert("authorization", "Bearer sk-client".parse().unwrap());
        client.insert("x-api-key", "sk-client".parse().unwrap());
        client.insert("user-agent", "claude-cli/2.0".parse().unwrap());
        client.insert("anthropic-beta", "tools-2024".parse().unwrap());
        client.insert("connection", "keep-alive".parse().unwrap());
        let out = request_headers(
            &client,
            Protocol::AnthropicMessages,
            AuthStyle::XApiKey,
            "sk-up",
        );
        assert_eq!(out["x-api-key"], "sk-up");
        assert!(out.get("authorization").is_none());
        assert!(out.get("connection").is_none());
        assert_eq!(out["user-agent"], "claude-cli/2.0");
        assert_eq!(out["anthropic-beta"], "tools-2024");
        assert_eq!(out["anthropic-version"], "2023-06-01");
    }
}
