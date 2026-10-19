use crate::auth::AdminUser;
use crate::error::ApiResult;
use crate::state::AdminState;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, SameSite};
use danmo_core::billing;
use danmo_core::protocol::{AuthStyle, Protocol, RelayMode};
use danmo_core::usage;
use danmo_core::services::{channel, log as log_svc, session, user as user_svc};
use danmo_core::AppError;
use serde::Deserialize;
use serde_json::{json, Value};
use time::Duration;

pub fn router(state: AdminState) -> Router {
    Router::new()
        .route("/api/admin/login", post(login))
        .route("/api/admin/logout", post(logout))
        .route("/api/admin/me", get(me))
        .route("/api/admin/overview", get(overview))
        .route("/api/admin/meta", get(meta))
        .route(
            "/api/admin/channels",
            get(list_channels).post(create_channel),
        )
        .route(
            "/api/admin/channels/{id}",
            put(update_channel).delete(delete_channel_handler),
        )
        .route("/api/admin/channels/{id}/status", put(set_channel_status))
        .route("/api/admin/users", get(list_users).post(create_user))
        .route("/api/admin/users/{id}", put(update_user))
        .route("/api/admin/users/{id}/quota", post(adjust_quota))
        .route(
            "/api/admin/model-prices",
            get(list_model_prices).put(upsert_model_price),
        )
        .route(
            "/api/admin/group-ratios",
            get(list_group_ratios).put(upsert_group_ratio),
        )
        .route("/api/admin/logs", get(list_logs))
        .with_state(state)
}

#[derive(Deserialize)]
struct LoginReq {
    username: String,
    password: String,
}

async fn login(
    State(state): State<AdminState>,
    Json(body): Json<LoginReq>,
) -> ApiResult<impl IntoResponse> {
    let user = user_svc::authenticate(&state.pool, &body.username, &body.password).await?;
    if !user.is_admin() {
        return Err(AppError::Forbidden("admin required".into()).into());
    }
    let sess =
        session::create_session(&state.pool, user.id, state.config.session_ttl_hours).await?;
    let cookie = Cookie::build(("danmo_session", sess.id))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(Duration::hours(state.config.session_ttl_hours))
        .build();

    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cookie.to_string().parse().unwrap());
    Ok((
        StatusCode::OK,
        headers,
        Json(json!({ "success": true, "data": user })),
    ))
}

async fn logout(
    State(state): State<AdminState>,
    jar: axum_extra::extract::CookieJar,
) -> ApiResult<impl IntoResponse> {
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

async fn me(AdminUser(user): AdminUser) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "success": true, "data": user })))
}

async fn overview(State(state): State<AdminState>, _admin: AdminUser) -> ApiResult<Json<Value>> {
    let stats = log_svc::overview(&state.pool).await?;
    Ok(Json(json!({ "success": true, "data": stats })))
}

/// Names accepted by channel and price fields, so the portal never hardcodes them.
async fn meta(_admin: AdminUser) -> ApiResult<Json<Value>> {
    let protocols: Vec<&str> = Protocol::ALL.iter().map(|p| p.as_str()).collect();
    let usage_adapters: Vec<Value> = usage::ADAPTERS
        .iter()
        .map(|a| json!({"name": a.name(), "protocol": a.protocol().as_str()}))
        .collect();
    Ok(Json(json!({
        "success": true,
        "data": {
            "protocols": protocols,
            "relay_modes": [RelayMode::Passthrough.as_str(), RelayMode::Convert.as_str()],
            "auth_styles": [
                AuthStyle::Bearer.as_str(),
                AuthStyle::XApiKey.as_str(),
                AuthStyle::XGoogApiKey.as_str(),
            ],
            "usage_adapters": usage_adapters,
        }
    })))
}

async fn list_channels(
    State(state): State<AdminState>,
    _admin: AdminUser,
) -> ApiResult<Json<Value>> {
    let channels = channel::list_channels(&state.pool).await?;
    Ok(Json(json!({ "success": true, "data": channels })))
}

#[derive(Deserialize)]
struct ChannelBody {
    name: String,
    #[serde(rename = "type", default = "default_channel_type")]
    type_: String,
    #[serde(default)]
    key: String,
    base_url: String,
    models: String,
    #[serde(default = "default_group")]
    group_name: String,
    #[serde(default)]
    priority: i64,
    #[serde(default = "default_weight")]
    weight: i64,
    #[serde(default = "default_enabled")]
    status: String,
    #[serde(default = "default_relay_mode")]
    relay_mode: String,
    #[serde(default = "default_upstream_protocol")]
    upstream_protocol: String,
    #[serde(default = "default_auth_style")]
    auth_style: String,
    #[serde(default)]
    model_mapping: String,
    #[serde(default = "default_true")]
    inject_stream_usage: bool,
}

impl ChannelBody {
    fn into_input(self) -> channel::ChannelInput {
        channel::ChannelInput {
            name: self.name,
            type_: self.type_,
            key: self.key,
            base_url: self.base_url,
            models: self.models,
            group_name: self.group_name,
            priority: self.priority,
            weight: self.weight,
            status: self.status,
            relay_mode: self.relay_mode,
            upstream_protocol: self.upstream_protocol,
            auth_style: self.auth_style,
            model_mapping: self.model_mapping,
            inject_stream_usage: self.inject_stream_usage,
        }
    }
}

fn default_relay_mode() -> String {
    "passthrough".into()
}
fn default_upstream_protocol() -> String {
    "openai_chat".into()
}
fn default_auth_style() -> String {
    "bearer".into()
}
fn default_true() -> bool {
    true
}
fn default_channel_type() -> String {
    "openai_compat".into()
}
fn default_group() -> String {
    "default".into()
}
fn default_weight() -> i64 {
    1
}
fn default_enabled() -> String {
    "enabled".into()
}

async fn create_channel(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Json(body): Json<ChannelBody>,
) -> ApiResult<Json<Value>> {
    if body.key.is_empty() {
        return Err(AppError::BadRequest("key is required".into()).into());
    }
    let ch = channel::create_channel(&state.pool, &state.config.encryption_key, body.into_input())
        .await?;
    Ok(Json(json!({ "success": true, "data": ch })))
}

async fn update_channel(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<ChannelBody>,
) -> ApiResult<Json<Value>> {
    let ch = channel::update_channel(
        &state.pool,
        &state.config.encryption_key,
        id,
        body.into_input(),
    )
    .await?;
    Ok(Json(json!({ "success": true, "data": ch })))
}

#[derive(Deserialize)]
struct StatusBody {
    status: String,
}

async fn set_channel_status(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<StatusBody>,
) -> ApiResult<Json<Value>> {
    let ch = channel::set_status(&state.pool, id, &body.status).await?;
    Ok(Json(json!({ "success": true, "data": ch })))
}

async fn delete_channel_handler(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    channel::delete_channel(&state.pool, id).await?;
    Ok(Json(json!({ "success": true })))
}

async fn list_users(State(state): State<AdminState>, _admin: AdminUser) -> ApiResult<Json<Value>> {
    let users = user_svc::list_users(&state.pool).await?;
    Ok(Json(json!({ "success": true, "data": users })))
}

#[derive(Deserialize)]
struct CreateUserBody {
    username: String,
    password: String,
    #[serde(default = "default_role")]
    role: String,
    #[serde(default)]
    quota: Option<i64>,
    #[serde(default = "default_group")]
    group_name: String,
}

fn default_role() -> String {
    "user".into()
}

async fn create_user(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Json(body): Json<CreateUserBody>,
) -> ApiResult<Json<Value>> {
    let quota = body.quota.unwrap_or(state.config.default_user_quota);
    let user = user_svc::create_user(
        &state.pool,
        &body.username,
        &body.password,
        &body.role,
        quota,
        &body.group_name,
    )
    .await?;
    Ok(Json(json!({ "success": true, "data": user })))
}

#[derive(Deserialize)]
struct UpdateUserBody {
    quota: Option<i64>,
    group_name: Option<String>,
    status: Option<String>,
    role: Option<String>,
}

async fn update_user(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<UpdateUserBody>,
) -> ApiResult<Json<Value>> {
    let user = user_svc::update_user(
        &state.pool,
        id,
        body.quota,
        body.group_name.as_deref(),
        body.status.as_deref(),
        body.role.as_deref(),
    )
    .await?;
    Ok(Json(json!({ "success": true, "data": user })))
}

#[derive(Deserialize)]
struct AdjustQuotaBody {
    delta: i64,
}

async fn adjust_quota(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<AdjustQuotaBody>,
) -> ApiResult<Json<Value>> {
    let user = user_svc::adjust_quota(&state.pool, id, body.delta).await?;
    Ok(Json(json!({ "success": true, "data": user })))
}

async fn list_model_prices(
    State(state): State<AdminState>,
    _admin: AdminUser,
) -> ApiResult<Json<Value>> {
    let prices = billing::list_model_prices(&state.pool).await?;
    Ok(Json(json!({ "success": true, "data": prices })))
}

#[derive(Deserialize)]
struct ModelPriceBody {
    model: String,
    model_ratio: f64,
    #[serde(default = "default_one")]
    completion_ratio: f64,
    #[serde(default = "default_one")]
    cache_read_ratio: f64,
    #[serde(default = "default_one")]
    cache_write_ratio: f64,
    #[serde(default)]
    search_price: f64,
    #[serde(default)]
    tiers: String,
    #[serde(default)]
    periods: String,
    #[serde(default)]
    native_usage: String,
}

fn default_one() -> f64 {
    1.0
}

async fn upsert_model_price(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Json(body): Json<ModelPriceBody>,
) -> ApiResult<Json<Value>> {
    let input = billing::ModelPriceInput {
        model: body.model,
        model_ratio: body.model_ratio,
        completion_ratio: body.completion_ratio,
        cache_read_ratio: body.cache_read_ratio,
        cache_write_ratio: body.cache_write_ratio,
        search_price: body.search_price,
        tiers: body.tiers,
        periods: body.periods,
        native_usage: body.native_usage,
    };
    let price = billing::upsert_model_price(&state.pool, &input).await?;
    Ok(Json(json!({ "success": true, "data": price })))
}

async fn list_group_ratios(
    State(state): State<AdminState>,
    _admin: AdminUser,
) -> ApiResult<Json<Value>> {
    let ratios = billing::list_group_ratios(&state.pool).await?;
    Ok(Json(json!({ "success": true, "data": ratios })))
}

#[derive(Deserialize)]
struct GroupRatioBody {
    group_name: String,
    ratio: f64,
}

async fn upsert_group_ratio(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Json(body): Json<GroupRatioBody>,
) -> ApiResult<Json<Value>> {
    let ratio = billing::upsert_group_ratio(&state.pool, &body.group_name, body.ratio).await?;
    Ok(Json(json!({ "success": true, "data": ratio })))
}

#[derive(Deserialize)]
struct LogQuery {
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
    user_id: Option<i64>,
}

fn default_limit() -> i64 {
    50
}

async fn list_logs(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Query(q): Query<LogQuery>,
) -> ApiResult<Json<Value>> {
    let logs = log_svc::list_logs(&state.pool, q.user_id, q.limit, q.offset).await?;
    Ok(Json(json!({ "success": true, "data": logs })))
}
