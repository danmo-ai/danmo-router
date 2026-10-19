use crate::crypto;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::Channel;
use crate::protocol::{AuthStyle, Protocol, RelayMode};

#[derive(Debug, Clone)]
pub struct ChannelInput {
    pub name: String,
    pub type_: String,
    pub key: String,
    pub base_url: String,
    pub models: String,
    pub group_name: String,
    pub priority: i64,
    pub weight: i64,
    pub status: String,
    pub relay_mode: String,
    pub upstream_protocol: String,
    pub auth_style: String,
    pub model_mapping: String,
    pub inject_stream_usage: bool,
}

impl Default for ChannelInput {
    fn default() -> Self {
        Self {
            name: String::new(),
            type_: "openai_compat".into(),
            key: String::new(),
            base_url: String::new(),
            models: String::new(),
            group_name: "default".into(),
            priority: 0,
            weight: 1,
            status: "enabled".into(),
            relay_mode: RelayMode::Passthrough.as_str().into(),
            upstream_protocol: Protocol::OpenaiChat.as_str().into(),
            auth_style: AuthStyle::Bearer.as_str().into(),
            model_mapping: String::new(),
            inject_stream_usage: true,
        }
    }
}

impl ChannelInput {
    fn validate(&self) -> AppResult<()> {
        if RelayMode::parse(&self.relay_mode).is_none() {
            return Err(AppError::BadRequest(format!(
                "invalid relay_mode '{}'",
                self.relay_mode
            )));
        }
        let protocol = Protocol::parse(&self.upstream_protocol).ok_or_else(|| {
            AppError::BadRequest(format!(
                "invalid upstream_protocol '{}'",
                self.upstream_protocol
            ))
        })?;
        if self.relay_mode == "convert" && !protocol.is_conversational() {
            return Err(AppError::BadRequest(format!(
                "{protocol} channels cannot convert; use passthrough"
            )));
        }
        if AuthStyle::parse(&self.auth_style).is_none() {
            return Err(AppError::BadRequest(format!(
                "invalid auth_style '{}'",
                self.auth_style
            )));
        }
        let mapping = self.model_mapping.trim();
        if !mapping.is_empty() {
            let parsed: serde_json::Map<String, serde_json::Value> = serde_json::from_str(mapping)
                .map_err(|e| {
                    AppError::BadRequest(format!("model_mapping must be a JSON object: {e}"))
                })?;
            if parsed.values().any(|v| !v.is_string()) {
                return Err(AppError::BadRequest(
                    "model_mapping values must be strings".into(),
                ));
            }
        }
        Ok(())
    }
}

pub async fn create_channel(
    pool: &DbPool,
    enc_key: &[u8; 32],
    input: ChannelInput,
) -> AppResult<Channel> {
    input.validate()?;
    let key_enc = crypto::encrypt_secret(enc_key, &input.key).map_err(AppError::Other)?;
    let result = sqlx::query(
        r#"
        INSERT INTO channels
            (name, type, key_enc, base_url, models, group_name, priority, weight, status,
             relay_mode, upstream_protocol, auth_style, model_mapping, inject_stream_usage)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&input.name)
    .bind(&input.type_)
    .bind(&key_enc)
    .bind(input.base_url.trim_end_matches('/'))
    .bind(&input.models)
    .bind(&input.group_name)
    .bind(input.priority)
    .bind(input.weight.max(1))
    .bind(&input.status)
    .bind(&input.relay_mode)
    .bind(&input.upstream_protocol)
    .bind(&input.auth_style)
    .bind(input.model_mapping.trim())
    .bind(i64::from(input.inject_stream_usage))
    .execute(pool)
    .await?;

    let id = result.last_insert_rowid();
    rebuild_abilities(pool, id, &input.group_name, &input.models).await?;
    get_by_id(pool, id).await
}

pub async fn update_channel(
    pool: &DbPool,
    enc_key: &[u8; 32],
    id: i64,
    input: ChannelInput,
) -> AppResult<Channel> {
    input.validate()?;
    let existing = get_by_id(pool, id).await?;
    let key_enc = if input.key.is_empty() {
        existing.key_enc
    } else {
        crypto::encrypt_secret(enc_key, &input.key).map_err(AppError::Other)?
    };

    sqlx::query(
        r#"
        UPDATE channels SET
            name = ?, type = ?, key_enc = ?, base_url = ?, models = ?,
            group_name = ?, priority = ?, weight = ?, status = ?,
            relay_mode = ?, upstream_protocol = ?, auth_style = ?, model_mapping = ?,
            inject_stream_usage = ?,
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(&input.name)
    .bind(&input.type_)
    .bind(&key_enc)
    .bind(input.base_url.trim_end_matches('/'))
    .bind(&input.models)
    .bind(&input.group_name)
    .bind(input.priority)
    .bind(input.weight.max(1))
    .bind(&input.status)
    .bind(&input.relay_mode)
    .bind(&input.upstream_protocol)
    .bind(&input.auth_style)
    .bind(input.model_mapping.trim())
    .bind(i64::from(input.inject_stream_usage))
    .bind(id)
    .execute(pool)
    .await?;

    rebuild_abilities(pool, id, &input.group_name, &input.models).await?;
    get_by_id(pool, id).await
}

pub async fn set_status(pool: &DbPool, id: i64, status: &str) -> AppResult<Channel> {
    sqlx::query("UPDATE channels SET status = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    get_by_id(pool, id).await
}

pub async fn delete_channel(pool: &DbPool, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM abilities WHERE channel_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM channels WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_by_id(pool: &DbPool, id: i64) -> AppResult<Channel> {
    sqlx::query_as("SELECT * FROM channels WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("channel not found".into()))
}

pub async fn list_channels(pool: &DbPool) -> AppResult<Vec<Channel>> {
    Ok(
        sqlx::query_as("SELECT * FROM channels ORDER BY priority DESC, id DESC")
            .fetch_all(pool)
            .await?,
    )
}

pub async fn decrypt_channel_key(enc_key: &[u8; 32], channel: &Channel) -> AppResult<String> {
    crypto::decrypt_secret(enc_key, &channel.key_enc).map_err(AppError::Other)
}

async fn rebuild_abilities(
    pool: &DbPool,
    channel_id: i64,
    group_name: &str,
    models: &str,
) -> AppResult<()> {
    sqlx::query("DELETE FROM abilities WHERE channel_id = ?")
        .bind(channel_id)
        .execute(pool)
        .await?;

    for model in models.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query(
            "INSERT OR IGNORE INTO abilities (channel_id, group_name, model) VALUES (?, ?, ?)",
        )
        .bind(channel_id)
        .bind(group_name)
        .bind(model)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn list_available_models(pool: &DbPool, group_name: &str) -> AppResult<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT a.model
        FROM abilities a
        INNER JOIN channels c ON c.id = a.channel_id
        WHERE a.group_name = ? AND c.status = 'enabled'
        ORDER BY a.model
        "#,
    )
    .bind(group_name)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}
