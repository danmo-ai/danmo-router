use axum::http::header::RETRY_AFTER;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use danmo_core::protocol::Protocol;
use danmo_core::AppError;
use serde_json::{json, Value};

pub struct RelayError(pub AppError);

impl From<AppError> for RelayError {
    fn from(value: AppError) -> Self {
        Self(value)
    }
}

impl RelayError {
    fn parts(&self) -> (StatusCode, String, &'static str) {
        match &self.0 {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone(), "bad_request"),
            AppError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.clone(), "unauthorized"),
            AppError::Forbidden(m) => (StatusCode::FORBIDDEN, m.clone(), "forbidden"),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone(), "not_found"),
            AppError::InsufficientQuota(m) => (
                StatusCode::PAYMENT_REQUIRED,
                m.clone(),
                "insufficient_quota",
            ),
            AppError::Conflict(m) => (StatusCode::CONFLICT, m.clone(), "conflict"),
            e @ AppError::Db(_) if e.is_db_busy() => (
                StatusCode::SERVICE_UNAVAILABLE,
                "database busy, retry shortly".into(),
                "database_busy",
            ),
            AppError::Db(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                e.to_string(),
                "database_error",
            ),
            AppError::Other(e) => (StatusCode::BAD_GATEWAY, e.to_string(), "upstream_error"),
        }
    }

    pub fn render(self, protocol: Protocol) -> Response {
        let (status, message, code) = self.parts();
        let response = protocol_error(protocol, status, &message, code);
        if status == StatusCode::SERVICE_UNAVAILABLE {
            with_retry_after(response)
        } else {
            response
        }
    }
}

fn with_retry_after(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(RETRY_AFTER, HeaderValue::from_static("1"));
    response
}

impl IntoResponse for RelayError {
    fn into_response(self) -> Response {
        self.render(Protocol::OpenaiChat)
    }
}

pub type RelayResult<T> = Result<T, RelayError>;

/// Error body in the shape the inbound protocol's SDKs expect.
pub fn protocol_error_body(
    protocol: Protocol,
    status: StatusCode,
    message: &str,
    code: &str,
) -> Value {
    match protocol {
        Protocol::AnthropicMessages => {
            let kind = match status.as_u16() {
                400 => "invalid_request_error",
                401 => "authentication_error",
                402 | 403 => "permission_error",
                404 => "not_found_error",
                429 => "rate_limit_error",
                529 => "overloaded_error",
                _ => "api_error",
            };
            json!({"type": "error", "error": {"type": kind, "message": message}})
        }
        Protocol::Gemini => {
            let kind = match status.as_u16() {
                400 => "INVALID_ARGUMENT",
                401 => "UNAUTHENTICATED",
                402 | 403 => "PERMISSION_DENIED",
                404 => "NOT_FOUND",
                429 => "RESOURCE_EXHAUSTED",
                _ => "INTERNAL",
            };
            json!({"error": {"code": status.as_u16(), "message": message, "status": kind}})
        }
        _ => json!({"error": {"message": message, "type": code, "code": code}}),
    }
}

pub fn rate_limited(protocol: Protocol) -> Response {
    with_retry_after(protocol_error(
        protocol,
        StatusCode::TOO_MANY_REQUESTS,
        "rate limit exceeded",
        "rate_limit_error",
    ))
}

pub fn protocol_error(
    protocol: Protocol,
    status: StatusCode,
    message: &str,
    code: &str,
) -> Response {
    (
        status,
        Json(protocol_error_body(protocol, status, message, code)),
    )
        .into_response()
}

/// Best-effort message from an upstream error body of any protocol.
pub fn upstream_message(body: &[u8]) -> String {
    let parsed: Option<Value> = serde_json::from_slice(body).ok();
    let found = parsed.as_ref().and_then(|v| {
        v.pointer("/error/message")
            .or_else(|| v.get("message"))
            .or_else(|| v.pointer("/base_resp/status_msg"))
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    found.unwrap_or_else(|| String::from_utf8_lossy(body).chars().take(500).collect())
}
