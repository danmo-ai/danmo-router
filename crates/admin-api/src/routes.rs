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
use danmo_core::dialect;
use danmo_core::protocol::{AuthStyle, Protocol, RelayMode};
use danmo_core::services::{
    channel, log as log_svc, redemption, session, token as token_svc, user as user_svc,
};
use danmo_core::usage;
use danmo_core::AppError;
use serde::Deserialize;
use serde_json::{json, Value};
use time::Duration;

pub fn router(state: AdminState) -> Router {
    log_svc::spawn_archive_task(state.pool.clone(), state.config.log_retention_days);
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
        .route("/api/admin/channels/{id}/test", post(test_channel))
        .route("/api/admin/users", get(list_users).post(create_user))
        .route("/api/admin/users/{id}", put(update_user))
        .route("/api/admin/users/{id}/quota", post(adjust_quota))
        .route("/api/admin/users/{id}/password", post(reset_password))
        .route("/api/admin/users/{id}/tokens", get(list_user_tokens))
        .route(
            "/api/admin/users/{id}/tokens/{token_id}/status",
            post(set_user_token_status),
        )
        .route(
            "/api/admin/users/{id}/tokens/{token_id}",
            axum::routing::delete(delete_user_token),
        )
        .route(
            "/api/admin/redemptions",
            get(list_redemptions).post(create_redemptions),
        )
        .route(
            "/api/admin/redemptions/{id}",
            axum::routing::delete(delete_redemption),
        )
        .route(
            "/api/admin/model-prices",
            get(list_model_prices).put(upsert_model_price),
        )
        .route(
            "/api/admin/group-ratios",
            get(list_group_ratios).put(upsert_group_ratio),
        )
        .route("/api/admin/logs", get(list_logs))
        .route("/api/admin/usage", get(usage))
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
            "dialects": dialect::DIALECTS,
        }
    })))
}

async fn list_channels(
    State(state): State<AdminState>,
    _admin: AdminUser,
) -> ApiResult<Json<Value>> {
    let channels = channel::list_views(&state.pool).await?;
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
    #[serde(default)]
    models: ModelsBody,
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
    #[serde(default = "default_true")]
    inject_stream_usage: bool,
    #[serde(default)]
    default_dialect: String,
    #[serde(default)]
    vendor: String,
}

/// Model rows, or a comma-separated list of names taking the channel's default dialect.
#[derive(Deserialize)]
#[serde(untagged)]
enum ModelsBody {
    Csv(String),
    Rows(Vec<channel::ChannelModelInput>),
}

impl Default for ModelsBody {
    fn default() -> Self {
        ModelsBody::Rows(Vec::new())
    }
}

impl ModelsBody {
    fn into_rows(self) -> Vec<channel::ChannelModelInput> {
        match self {
            ModelsBody::Csv(s) => channel::ChannelModelInput::list(&s),
            ModelsBody::Rows(rows) => rows,
        }
    }
}

impl ChannelBody {
    fn into_input(self) -> channel::ChannelInput {
        channel::ChannelInput {
            name: self.name,
            type_: self.type_,
            key: self.key,
            base_url: self.base_url,
            models: self.models.into_rows(),
            group_name: self.group_name,
            priority: self.priority,
            weight: self.weight,
            status: self.status,
            relay_mode: self.relay_mode,
            upstream_protocol: self.upstream_protocol,
            auth_style: self.auth_style,
            inject_stream_usage: self.inject_stream_usage,
            default_dialect: self.default_dialect,
            vendor: self.vendor,
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
    let view = channel::get_view(&state.pool, ch.id).await?;
    Ok(Json(json!({ "success": true, "data": view })))
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
    let view = channel::get_view(&state.pool, ch.id).await?;
    Ok(Json(json!({ "success": true, "data": view })))
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
    /// A JSON object from protocol to dialect, or that object serialized as a string.
    #[serde(default)]
    native_dialects: Value,
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
        native_dialects: match body.native_dialects {
            Value::Null => String::new(),
            Value::String(s) => s,
            other => other.to_string(),
        },
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
    token_id: Option<i64>,
    channel_id: Option<i64>,
    model: Option<String>,
    status: Option<String>,
    start: Option<String>,
    end: Option<String>,
}

fn default_limit() -> i64 {
    50
}

fn admin_log_filter(q: &LogQuery) -> log_svc::LogFilter {
    log_svc::LogFilter {
        user_id: q.user_id,
        token_id: q.token_id,
        channel_id: q.channel_id,
        model: q.model.clone(),
        status: q.status.clone(),
        start: q.start.clone(),
        end: q.end.clone(),
    }
}

async fn list_logs(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Query(q): Query<LogQuery>,
) -> ApiResult<Json<Value>> {
    let filter = admin_log_filter(&q);
    let logs = log_svc::list_logs(
        &state.pool,
        &log_svc::LogQuery {
            user_id: filter.user_id,
            token_id: filter.token_id,
            channel_id: filter.channel_id,
            model: filter.model,
            status: filter.status,
            start: filter.start,
            end: filter.end,
            limit: q.limit,
            offset: q.offset,
        },
    )
    .await?;
    Ok(Json(json!({ "success": true, "data": logs })))
}

#[derive(Deserialize)]
struct UsageQuery {
    user_id: Option<i64>,
    token_id: Option<i64>,
    channel_id: Option<i64>,
    model: Option<String>,
    status: Option<String>,
    start: Option<String>,
    end: Option<String>,
}

async fn usage(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Query(q): Query<UsageQuery>,
) -> ApiResult<Json<Value>> {
    let filter = log_svc::LogFilter {
        user_id: q.user_id,
        token_id: q.token_id,
        channel_id: q.channel_id,
        model: q.model,
        status: q.status,
        start: q.start,
        end: q.end,
    };
    let report =
        log_svc::usage_report(&state.pool, &filter, state.config.log_retention_days).await?;
    Ok(Json(json!({ "success": true, "data": report })))
}

async fn test_channel(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let result = crate::probe::test_channel(&state.pool, &state.config, id).await?;
    Ok(Json(json!({ "success": true, "data": result })))
}

#[derive(Deserialize)]
struct ResetPasswordBody {
    password: String,
}

async fn reset_password(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
    Json(body): Json<ResetPasswordBody>,
) -> ApiResult<Json<Value>> {
    user_svc::set_password(&state.pool, id, &body.password).await?;
    Ok(Json(json!({ "success": true })))
}

async fn list_user_tokens(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let _ = user_svc::get_by_id(&state.pool, id).await?;
    let tokens = token_svc::list_by_user(&state.pool, id).await?;
    Ok(Json(json!({ "success": true, "data": tokens })))
}

#[derive(Deserialize)]
struct TokenStatusBody {
    status: String,
}

async fn set_user_token_status(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Path((id, token_id)): Path<(i64, i64)>,
    Json(body): Json<TokenStatusBody>,
) -> ApiResult<Json<Value>> {
    let token = token_svc::set_status(&state.pool, token_id, id, &body.status).await?;
    Ok(Json(json!({ "success": true, "data": token })))
}

async fn delete_user_token(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Path((id, token_id)): Path<(i64, i64)>,
) -> ApiResult<Json<Value>> {
    token_svc::delete_token(&state.pool, token_id, id).await?;
    Ok(Json(json!({ "success": true })))
}

async fn list_redemptions(
    State(state): State<AdminState>,
    _admin: AdminUser,
) -> ApiResult<Json<Value>> {
    let rows = redemption::list(&state.pool).await?;
    Ok(Json(json!({ "success": true, "data": rows })))
}

#[derive(Deserialize)]
struct CreateRedemptionsBody {
    #[serde(default)]
    name: String,
    quota: i64,
    #[serde(default = "default_count")]
    count: i64,
}

fn default_count() -> i64 {
    1
}

async fn create_redemptions(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Json(body): Json<CreateRedemptionsBody>,
) -> ApiResult<Json<Value>> {
    let rows = redemption::create_batch(&state.pool, &body.name, body.quota, body.count).await?;
    Ok(Json(json!({ "success": true, "data": rows })))
}

async fn delete_redemption(
    State(state): State<AdminState>,
    _admin: AdminUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    redemption::delete_unused(&state.pool, id).await?;
    Ok(Json(json!({ "success": true })))
}
