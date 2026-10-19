use serde::{Deserialize, Serialize};
use std::fmt;

/// Wire protocol of a request or response. Inbound protocol is decided by the request path;
/// a channel declares the protocol its upstream speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    OpenaiChat,
    OpenaiResponses,
    AnthropicMessages,
    Gemini,
    Embeddings,
}

impl Protocol {
    pub const ALL: [Protocol; 5] = [
        Protocol::OpenaiChat,
        Protocol::OpenaiResponses,
        Protocol::AnthropicMessages,
        Protocol::Gemini,
        Protocol::Embeddings,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Protocol::OpenaiChat => "openai_chat",
            Protocol::OpenaiResponses => "openai_responses",
            Protocol::AnthropicMessages => "anthropic_messages",
            Protocol::Gemini => "gemini",
            Protocol::Embeddings => "embeddings",
        }
    }

    pub fn parse(s: &str) -> Option<Protocol> {
        Protocol::ALL.into_iter().find(|p| p.as_str() == s)
    }

    /// Conversation protocols can be converted into each other through the IR.
    pub fn is_conversational(self) -> bool {
        !matches!(self, Protocol::Embeddings)
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayMode {
    Passthrough,
    Convert,
}

impl RelayMode {
    pub fn as_str(self) -> &'static str {
        match self {
            RelayMode::Passthrough => "passthrough",
            RelayMode::Convert => "convert",
        }
    }

    pub fn parse(s: &str) -> Option<RelayMode> {
        match s {
            "passthrough" => Some(RelayMode::Passthrough),
            "convert" => Some(RelayMode::Convert),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthStyle {
    Bearer,
    XApiKey,
    XGoogApiKey,
}

impl AuthStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            AuthStyle::Bearer => "bearer",
            AuthStyle::XApiKey => "x_api_key",
            AuthStyle::XGoogApiKey => "x_goog_api_key",
        }
    }

    pub fn parse(s: &str) -> Option<AuthStyle> {
        match s {
            "bearer" => Some(AuthStyle::Bearer),
            "x_api_key" => Some(AuthStyle::XApiKey),
            "x_goog_api_key" => Some(AuthStyle::XGoogApiKey),
            _ => None,
        }
    }
}
