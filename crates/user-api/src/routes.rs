use crate::auth::AuthUser;
use crate::error::ApiResult;
use crate::state::UserState;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use danmo_core::services::{log as log_svc, session, token as token_svc, user as user_svc};
use serde::Deserialize;
use serde_json::{json, Value};
use time::Duration;

pub fn router(state: UserState) -> Router {
    Router::new()
        .route("/api/user/register", post(register))
        .route("/api/user/login", post(login))
        .route("/api/user/logout", post(logout))
        .route("/api/user/me", get(me))
        .route("/api/user/tokens", get(list_tokens).post(create_token))
        .route("/api/user/tokens/{id}/status", post(set_token_status))
        .route("/api/user/tokens/{id}", axum::routing::delete(delete_token))
        .route("/api/user/logs", get(list_logs))
        .with_state(state)
}

#[derive(Deserialize)]
struct AuthBody {
    username: String,
    password: String,
}

async fn register(
    State(state): State<UserState>,
    Json(body): Json<AuthBody>,
) -> ApiResult<impl IntoResponse> {
    let user = user_svc::create_user(
        &state.pool,
        &body.username,
        &body.password,
        "user",
        state.config.default_user_quota,
        "default",
    )
    .await?;
    let sess =
        session::create_session(&state.pool, user.id, state.config.session_ttl_hours).await?;
    Ok(with_session_cookie(
        state.config.session_ttl_hours,
        &sess.id,
        json!({ "success": true, "data": user }),
    ))
}

async fn login(
    State(state): State<UserState>,
    Json(body): Json<AuthBody>,
) -> ApiResult<impl IntoResponse> {
    let user = user_svc::authenticate(&state.pool, &body.username, &body.password).await?;
    let sess =
        session::create_session(&state.pool, user.id, state.config.session_ttl_hours).await?;
    Ok(with_session_cookie(
        state.config.session_ttl_hours,
        &sess.id,
        json!({ "success": true, "data": user }),
    ))
}

async fn logout(State(state): State<UserState>, jar: CookieJar) -> ApiResult<impl IntoResponse> {
    if let Some(c) = jar.get("danmo_session") {
        let _ = session::delete_session(&state.pool, c.value()).await;
    }
    let cookie = Cookie::build(("danmo_session", ""))
        .path("/")
        .http_only(true)
        .max_age(Duration::seconds(0))
        .build();
    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cookie.to_string().parse().unwrap());
    Ok((StatusCode::OK, headers, Json(json!({ "success": true }))))
}

async fn me(AuthUser(user): AuthUser) -> ApiResult<Json<Value>> {
    Ok(Json(json!({
        "success": true,
        "data": {
            "id": user.id,
            "username": user.username,
            "role": user.role,
            "quota": user.quota,
            "used_quota": user.used_quota,
            "remaining_quota": user.remaining_quota(),
            "group_name": user.group_name,
            "status": user.status,
            "created_at": user.created_at
        }
    })))
}

async fn list_tokens(
    State(state): State<UserState>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Value>> {
    let tokens = token_svc::list_by_user(&state.pool, user.id).await?;
    Ok(Json(json!({ "success": true, "data": tokens })))
}

#[derive(Deserialize)]
struct CreateTokenBody {
    name: String,
    #[serde(default = "default_remain")]
    remain_quota: i64,
    #[serde(default = "default_true")]
    unlimited_quota: bool,
    expired_at: Option<String>,
    #[serde(default)]
    models: String,
    group_name: Option<String>,
}

fn default_remain() -> i64 {
    -1
}
fn default_true() -> bool {
    true
}

async fn create_token(
    State(state): State<UserState>,
    AuthUser(user): AuthUser,
    Json(body): Json<CreateTokenBody>,
) -> ApiResult<Json<Value>> {
    let created = token_svc::create_token(
        &state.pool,
        user.id,
        &body.name,
        body.remain_quota,
        body.unlimited_quota,
        body.expired_at.as_deref(),
        &body.models,
        body.group_name.as_deref(),
    )
    .await?;
    Ok(Json(json!({
        "success": true,
        "data": {
            "token": created.token,
            "key": created.raw_key
        }
    })))
}

#[derive(Deserialize)]
struct TokenStatusBody {
    status: String,
}

async fn set_token_status(
    State(state): State<UserState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<TokenStatusBody>,
) -> ApiResult<Json<Value>> {
    let token = token_svc::set_status(&state.pool, id, user.id, &body.status).await?;
    Ok(Json(json!({ "success": true, "data": token })))
}

async fn delete_token(
    State(state): State<UserState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    token_svc::delete_token(&state.pool, id, user.id).await?;
    Ok(Json(json!({ "success": true })))
}

#[derive(Deserialize)]
struct LogQuery {
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_limit() -> i64 {
    50
}

async fn list_logs(
    State(state): State<UserState>,
    AuthUser(user): AuthUser,
    Query(q): Query<LogQuery>,
) -> ApiResult<Json<Value>> {
    let logs = log_svc::list_logs(&state.pool, Some(user.id), q.limit, q.offset).await?;
    Ok(Json(json!({ "success": true, "data": logs })))
}

fn with_session_cookie(
    ttl_hours: i64,
    session_id: &str,
    body: Value,
) -> (StatusCode, HeaderMap, Json<Value>) {
    let cookie = Cookie::build(("danmo_session", session_id.to_string()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(Duration::hours(ttl_hours))
        .build();
    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cookie.to_string().parse().unwrap());
    (StatusCode::OK, headers, Json(body))
}
