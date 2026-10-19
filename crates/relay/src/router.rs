use crate::error::RelayError;
use crate::forward::{self, Inbound};
use crate::state::RelayState;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
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
        .with_state(state)
}

type QueryMap = Query<HashMap<String, String>>;

async fn models(
    State(state): State<RelayState>,
    Query(query): QueryMap,
    headers: HeaderMap,
) -> Response {
    let result = match api_key(&headers, &query) {
        Ok(key) => forward::list_models(&state, &key).await,
        Err(e) => Err(e),
    };
    result.unwrap_or_else(IntoResponse::into_response)
}

async fn chat_completions(
    State(state): State<RelayState>,
    Query(query): QueryMap,
    headers: HeaderMap,
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
    )
    .await
}

async fn responses(
    State(state): State<RelayState>,
    Query(query): QueryMap,
    headers: HeaderMap,
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
    )
    .await
}

async fn messages(
    State(state): State<RelayState>,
    Query(query): QueryMap,
    headers: HeaderMap,
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
    )
    .await
}

async fn embeddings(
    State(state): State<RelayState>,
    Query(query): QueryMap,
    headers: HeaderMap,
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
    )
    .await
}

/// `/v1beta/models/{model}:generateContent` and `:streamGenerateContent`.
async fn gemini(
    State(state): State<RelayState>,
    Path(target): Path<String>,
    Query(query): QueryMap,
    headers: HeaderMap,
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
    )
    .await
}

async fn relay(
    state: &RelayState,
    protocol: Protocol,
    headers: HeaderMap,
    query: &HashMap<String, String>,
    body: Bytes,
    path_model: Option<String>,
    path_stream: bool,
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
    };
    forward::handle(state, inbound)
        .await
        .unwrap_or_else(|e| e.render(protocol))
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
