use crate::admit::{AdmitPermit, TakePermit};
use crate::error::{self, RelayError};
use crate::forward::{self, Inbound};
use crate::state::RelayState;
use axum::body::Bytes;
use axum::extract::{Path, Query, Request, State};
use axum::http::HeaderMap;
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use danmo_core::crypto::{self, ApiKeyClass};
use danmo_core::protocol::Protocol;
use danmo_core::AppError;
use std::collections::HashMap;

pub fn router(state: RelayState) -> Router {
    Router::new()
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/responses", post(responses))
        .route("/v1/messages", post(messages))
        .route("/v1/embeddings", post(embeddings))
        .route("/v1beta/models/{target}", post(gemini))
        .route_layer(from_fn_with_state(state.clone(), admit))
        .with_state(state)
}

/// Runs before the handler reads the body. Forged keys stop here and spend nothing.
async fn admit(State(state): State<RelayState>, mut req: Request, next: Next) -> Response {
    let protocol = protocol_of(req.uri().path());
    let presented = presented_key(req.headers(), req.uri().query());
    let class = crypto::classify_api_key(&state.config.encryption_key, &presented);
    let permit = match class {
        ApiKeyClass::Invalid => {
            return error::protocol_error(
                protocol,
                axum::http::StatusCode::UNAUTHORIZED,
                "invalid API key",
                "unauthorized",
            );
        }
        ApiKeyClass::Verified { user_id } => state.admit.acquire_verified(user_id),
        ApiKeyClass::Legacy => state.admit.acquire_legacy(),
    };
    match permit {
        Ok(permit) => {
            req.extensions_mut().insert(permit);
            next.run(req).await
        }
        Err(()) => error::rate_limited(protocol),
    }
}

fn protocol_of(path: &str) -> Protocol {
    match path {
        "/v1/messages" => Protocol::AnthropicMessages,
        "/v1/responses" => Protocol::OpenaiResponses,
        "/v1/embeddings" => Protocol::Embeddings,
        p if p.starts_with("/v1beta/") => Protocol::Gemini,
        _ => Protocol::OpenaiChat,
    }
}

type QueryMap = Query<HashMap<String, String>>;

async fn models(
    State(state): State<RelayState>,
    Query(query): QueryMap,
    headers: HeaderMap,
    TakePermit(permit): TakePermit,
) -> Response {
    let result = match api_key(&headers, &query) {
        Ok(key) => forward::list_models(&state, &key, permit).await,
        Err(e) => Err(e),
    };
    result.unwrap_or_else(IntoResponse::into_response)
}

async fn chat_completions(
    State(state): State<RelayState>,
    Query(query): QueryMap,
    headers: HeaderMap,
    TakePermit(permit): TakePermit,
    body: Bytes,
) -> Response {
    relay(
        &state,
        Protocol::OpenaiChat,
        headers,
        &query,
        body,
        None,
        false,
        permit,
    )
    .await
}

async fn responses(
    State(state): State<RelayState>,
    Query(query): QueryMap,
    headers: HeaderMap,
    TakePermit(permit): TakePermit,
    body: Bytes,
) -> Response {
    relay(
        &state,
        Protocol::OpenaiResponses,
        headers,
        &query,
        body,
        None,
        false,
        permit,
    )
    .await
}

async fn messages(
    State(state): State<RelayState>,
    Query(query): QueryMap,
    headers: HeaderMap,
    TakePermit(permit): TakePermit,
    body: Bytes,
) -> Response {
    relay(
        &state,
        Protocol::AnthropicMessages,
        headers,
        &query,
        body,
        None,
        false,
        permit,
    )
    .await
}

async fn embeddings(
    State(state): State<RelayState>,
    Query(query): QueryMap,
    headers: HeaderMap,
    TakePermit(permit): TakePermit,
    body: Bytes,
) -> Response {
    relay(
        &state,
        Protocol::Embeddings,
        headers,
        &query,
        body,
        None,
        false,
        permit,
    )
    .await
}

/// `/v1beta/models/{model}:generateContent` and `:streamGenerateContent`.
async fn gemini(
    State(state): State<RelayState>,
    Path(target): Path<String>,
    Query(query): QueryMap,
    headers: HeaderMap,
    TakePermit(permit): TakePermit,
    body: Bytes,
) -> Response {
    let (model, stream) = match target.rsplit_once(':') {
        Some((m, "generateContent")) => (m.to_string(), false),
        Some((m, "streamGenerateContent")) => (m.to_string(), true),
        _ => {
            return RelayError(AppError::NotFound(format!("unsupported method '{target}'")))
                .render(Protocol::Gemini)
        }
    };
    relay(
        &state,
        Protocol::Gemini,
        headers,
        &query,
        body,
        Some(model),
        stream,
        permit,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn relay(
    state: &RelayState,
    protocol: Protocol,
    headers: HeaderMap,
    query: &HashMap<String, String>,
    body: Bytes,
    path_model: Option<String>,
    path_stream: bool,
    permit: AdmitPermit,
) -> Response {
    let api_key = match api_key(&headers, query) {
        Ok(k) => k,
        Err(e) => return e.render(protocol),
    };
    let inbound = Inbound {
        protocol,
        api_key,
        headers,
        body,
        path_model,
        path_stream,
        permit,
    };
    forward::handle(state, inbound)
        .await
        .unwrap_or_else(|e| e.render(protocol))
}

fn presented_key(headers: &HeaderMap, query: Option<&str>) -> String {
    ["authorization", "x-api-key", "x-goog-api-key"]
        .iter()
        .find_map(|name| headers.get(*name).and_then(|v| v.to_str().ok()))
        .map(str::to_string)
        .or_else(|| query.and_then(query_key))
        .unwrap_or_default()
}

fn query_key(query: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == "key").then(|| percent_decode(value))
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// OpenAI SDKs send `Authorization: Bearer`, Anthropic SDKs `x-api-key`, Gemini SDKs
/// `x-goog-api-key` or `?key=`.
fn api_key(headers: &HeaderMap, query: &HashMap<String, String>) -> Result<String, RelayError> {
    ["authorization", "x-api-key", "x-goog-api-key"]
        .iter()
        .find_map(|h| headers.get(*h).and_then(|v| v.to_str().ok()))
        .map(str::to_string)
        .or_else(|| query.get("key").cloned())
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| RelayError(AppError::Unauthorized("missing API key".into())))
}
