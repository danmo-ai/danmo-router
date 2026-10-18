use crate::error::RelayResult;
use crate::forward::{self, RelayRequest};
use crate::state::RelayState;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
use danmo_core::AppError;

pub fn router(state: RelayState) -> Router {
    Router::new()
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/embeddings", post(embeddings))
        .with_state(state)
}

async fn models(State(state): State<RelayState>, headers: HeaderMap) -> RelayResult<Response> {
    let auth = authorization(&headers)?;
    forward::list_models(&state, &auth).await
}

async fn chat_completions(
    State(state): State<RelayState>,
    headers: HeaderMap,
    body: Bytes,
) -> RelayResult<Response> {
    relay_path(&state, headers, body, "/v1/chat/completions").await
}

async fn embeddings(
    State(state): State<RelayState>,
    headers: HeaderMap,
    body: Bytes,
) -> RelayResult<Response> {
    relay_path(&state, headers, body, "/v1/embeddings").await
}

async fn relay_path(
    state: &RelayState,
    headers: HeaderMap,
    body: Bytes,
    path: &str,
) -> RelayResult<Response> {
    let auth = authorization(&headers)?;
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    forward::handle_openai_compat(
        state,
        RelayRequest {
            path: path.to_string(),
            auth_header: auth,
            body,
            content_type,
        },
    )
    .await
}

fn authorization(headers: &HeaderMap) -> Result<String, crate::error::RelayError> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| {
            crate::error::RelayError(AppError::Unauthorized(
                "missing Authorization header".into(),
            ))
        })
}

#[allow(dead_code)]
fn _status() -> StatusCode {
    StatusCode::OK
}
