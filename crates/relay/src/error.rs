use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use danmo_core::AppError;
use serde_json::json;

pub struct RelayError(pub AppError);

impl From<AppError> for RelayError {
    fn from(value: AppError) -> Self {
        Self(value)
    }
}

impl IntoResponse for RelayError {
    fn into_response(self) -> Response {
        let (status, message, code) = match &self.0 {
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
            AppError::Db(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                e.to_string(),
                "database_error",
            ),
            AppError::Other(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                e.to_string(),
                "internal_error",
            ),
        };

        (
            status,
            Json(json!({
                "error": {
                    "message": message,
                    "type": code,
                    "code": code
                }
            })),
        )
            .into_response()
    }
}

pub type RelayResult<T> = Result<T, RelayError>;
