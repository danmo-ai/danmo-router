use crate::error::ApiError;
use crate::state::AdminState;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use danmo_core::services::session;
use danmo_core::{AppError, User};

pub struct AdminUser(pub User);

impl FromRequestParts<AdminState> for AdminUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AdminState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let session_id = jar
            .get("danmo_session")
            .map(|c| c.value().to_string())
            .ok_or_else(|| ApiError(AppError::Unauthorized("not logged in".into())))?;

        let user = session::get_user_by_session(&state.pool, &session_id).await?;
        if !user.is_admin() {
            return Err(ApiError(AppError::Forbidden("admin required".into())));
        }
        if !user.is_active() {
            return Err(ApiError(AppError::Forbidden("user disabled".into())));
        }
        Ok(AdminUser(user))
    }
}
