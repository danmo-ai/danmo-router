use crate::auth::AuthUser;
use crate::error::ApiResult;
use crate::state::UserState;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, SameSite};
use axum_extra::extract::CookieJar;
use danmo_core::models::User;
use danmo_core::services::{
    catalog, log as log_svc, redemption, session, token as token_svc, user as user_svc,
};
use serde::Deserialize;
use serde_json::{json, Value};
use time::Duration;

pub fn router(state: UserState) -> Router {
    Router::new()
        .route("/api/user/captcha", get(captcha))
        .route("/api/user/register", post(register))
        .route("/api/user/login", post(login))
        .route("/api/user/logout", post(logout))
        .route("/api/user/me", get(me))
        .route("/api/user/meta", get(meta))
        .route("/api/user/tokens", get(list_tokens).post(create_token))
        .route("/api/user/tokens/{id}/status", post(set_token_status))
        .route(
            "/api/user/tokens/{id}",
            put(update_token).delete(delete_token),
        )
        .route("/api/user/logs", get(list_logs))
        .route("/api/user/redeem", post(redeem))
        .route("/api/user/password", post(change_password))
        .route("/api/user/models", get(list_models))
        .with_state(state)
}

#[derive(Deserialize)]
struct AuthBody {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct RegisterBody {
    username: String,
    password: String,
    captcha_token: String,
    captcha_answer: String,
}

async fn captcha(State(state): State<UserState>) -> ApiResult<impl IntoResponse> {
    let challenge = danmo_core::captcha::issue(&state.config.encryption_key)?;
    Ok(Json(json!({
        "success": true,
        "data": {
            "token": challenge.token,
            "image_svg": challenge.image_svg,
        }
    })))
}

async fn register(
    State(state): State<UserState>,
    Json(body): Json<RegisterBody>,
) -> ApiResult<impl IntoResponse> {
    let nonce = danmo_core::captcha::verify(
        &state.config.encryption_key,
        &body.captcha_token,
        &body.captcha_answer,
    )?;
    if state.captcha_replay.consume(&nonce) {
        return Err(danmo_core::AppError::BadRequest("验证码无效或已过期".into()).into());
    }

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

fn profile(user: &User) -> Value {
    json!({
        "id": user.id,
        "username": user.username,
        "role": user.role,
        "quota": user.quota,
        "used_quota": user.used_quota,
        "remaining_quota": user.remaining_quota(),
        "group_name": user.group_name,
        "status": user.status,
        "created_at": user.created_at
    })
}

async fn me(AuthUser(user): AuthUser) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "success": true, "data": profile(&user) })))
}

async fn meta(State(state): State<UserState>, _user: AuthUser) -> ApiResult<Json<Value>> {
    let configured = state.config.public_relay_url.trim();
    let relay_base_url = if configured.is_empty() {
        "http://127.0.0.1:3000/v1".to_string()
    } else {
        configured.trim_end_matches('/').to_string()
    };
    Ok(Json(json!({
        "success": true,
        "data": { "relay_base_url": relay_base_url }
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
        &state.config.encryption_key,
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

#[derive(Deserialize)]
struct UpdateTokenBody {
    name: String,
    #[serde(default = "default_remain")]
    remain_quota: i64,
    #[serde(default)]
    unlimited_quota: bool,
    expired_at: Option<String>,
    #[serde(default)]
    models: String,
    group_name: Option<String>,
}

async fn update_token(
    State(state): State<UserState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
    Json(body): Json<UpdateTokenBody>,
) -> ApiResult<Json<Value>> {
    let token = token_svc::update_token(
        &state.pool,
        id,
        user.id,
        token_svc::TokenUpdate {
            name: body.name,
            remain_quota: body.remain_quota,
            unlimited_quota: body.unlimited_quota,
            expired_at: body.expired_at,
            models: body.models,
            group_name: body.group_name,
        },
    )
    .await?;
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
    model: Option<String>,
    status: Option<String>,
    channel_id: Option<i64>,
}

fn default_limit() -> i64 {
    50
}

async fn list_logs(
    State(state): State<UserState>,
    AuthUser(user): AuthUser,
    Query(q): Query<LogQuery>,
) -> ApiResult<Json<Value>> {
    let logs = log_svc::list_logs(
        &state.pool,
        &log_svc::LogQuery {
            user_id: Some(user.id),
            model: q.model,
            status: q.status,
            channel_id: q.channel_id,
            limit: q.limit,
            offset: q.offset,
        },
    )
    .await?;
    Ok(Json(json!({ "success": true, "data": logs })))
}

#[derive(Deserialize)]
struct RedeemBody {
    code: String,
}

async fn redeem(
    State(state): State<UserState>,
    AuthUser(user): AuthUser,
    Json(body): Json<RedeemBody>,
) -> ApiResult<Json<Value>> {
    let updated = redemption::redeem(&state.pool, user.id, &body.code).await?;
    Ok(Json(json!({ "success": true, "data": profile(&updated) })))
}

#[derive(Deserialize)]
struct PasswordBody {
    old_password: String,
    password: String,
}

async fn change_password(
    State(state): State<UserState>,
    AuthUser(user): AuthUser,
    Json(body): Json<PasswordBody>,
) -> ApiResult<Json<Value>> {
    user_svc::change_password(&state.pool, user.id, &body.old_password, &body.password).await?;
    Ok(Json(json!({ "success": true })))
}

async fn list_models(
    State(state): State<UserState>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Value>> {
    let catalog = catalog::list_for_group(&state.pool, &user.group_name).await?;
    Ok(Json(json!({ "success": true, "data": catalog })))
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
