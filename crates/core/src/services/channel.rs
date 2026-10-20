use crate::crypto;
use crate::db::{self, DbPool, DbTx, SqlEngine};
use crate::dialect;
use crate::error::{AppError, AppResult};
use crate::models::{Channel, ChannelModel};
use crate::protocol::{AuthStyle, Protocol, RelayMode};
use serde::{Deserialize, Serialize};

/// One model row of a channel. An empty `upstream_dialect` takes the channel's default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelModelInput {
    pub model: String,
    #[serde(default)]
    pub upstream_model: String,
    #[serde(default)]
    pub upstream_dialect: String,
}

impl ChannelModelInput {
    pub fn new(model: &str) -> Self {
        Self {
            model: model.to_string(),
            ..Self::default()
        }
    }

    pub fn with(model: &str, upstream_model: &str, upstream_dialect: &str) -> Self {
        Self {
            model: model.to_string(),
            upstream_model: upstream_model.to_string(),
            upstream_dialect: upstream_dialect.to_string(),
        }
    }

    /// Rows from a comma-separated model list, each with the channel's default dialect.
    pub fn list(csv: &str) -> Vec<Self> {
        csv.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(Self::new)
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct ChannelInput {
    pub name: String,
    pub type_: String,
    pub key: String,
    pub base_url: String,
    pub models: Vec<ChannelModelInput>,
    pub group_name: String,
    pub priority: i64,
    pub weight: i64,
    pub status: String,
    pub relay_mode: String,
    pub upstream_protocol: String,
    pub auth_style: String,
    pub inject_stream_usage: bool,
    pub default_dialect: String,
    pub vendor: String,
}

impl Default for ChannelInput {
    fn default() -> Self {
        Self {
            name: String::new(),
            type_: "openai_compat".into(),
            key: String::new(),
            base_url: String::new(),
            models: Vec::new(),
            group_name: "default".into(),
            priority: 0,
            weight: 1,
            status: "enabled".into(),
            relay_mode: RelayMode::Passthrough.as_str().into(),
            upstream_protocol: Protocol::OpenaiChat.as_str().into(),
            auth_style: AuthStyle::Bearer.as_str().into(),
            inject_stream_usage: true,
            default_dialect: dialect::STANDARD.into(),
            vendor: String::new(),
        }
    }
}

impl ChannelInput {
    /// Validates the input and returns the model rows with names trimmed and dialects resolved.
    fn validate(&self) -> AppResult<Vec<ChannelModelInput>> {
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
        let default = match self.default_dialect.trim() {
            "" => dialect::STANDARD,
            d => d,
        };
        dialect::validate(protocol, default)?;

        let mut rows: Vec<ChannelModelInput> = Vec::with_capacity(self.models.len());
        for m in &self.models {
            let model = m.model.trim();
            if model.is_empty() {
                continue;
            }
            if rows.iter().any(|r| r.model == model) {
                return Err(AppError::BadRequest(format!("duplicate model '{model}'")));
            }
            let d = match m.upstream_dialect.trim() {
                "" => default,
                d => d,
            };
            dialect::validate(protocol, d)?;
            let upstream = m.upstream_model.trim();
            rows.push(ChannelModelInput {
                model: model.to_string(),
                upstream_model: if upstream == model {
                    String::new()
                } else {
                    upstream.to_string()
                },
                upstream_dialect: d.to_string(),
            });
        }
        Ok(rows)
    }
}

/// A channel with its model rows, as shown to admins.
#[derive(Debug, Clone, Serialize)]
pub struct ChannelView {
    #[serde(flatten)]
    pub channel: Channel,
    pub models: Vec<ChannelModel>,
}

pub async fn create_channel(
    pool: &DbPool,
    enc_key: &[u8; 32],
    input: ChannelInput,
) -> AppResult<Channel> {
    let rows = input.validate()?;
    let key_enc = crypto::encrypt_secret(enc_key, &input.key).map_err(AppError::Other)?;
    let engine = db::engine(pool);
    let now = db::now();
    let mut tx = db::begin_write(pool).await?;
    let sql = format!(
        r#"
        INSERT INTO channels
            (name, type, key_enc, base_url, group_name, priority, weight, status,
             relay_mode, upstream_protocol, auth_style, inject_stream_usage,
             default_dialect, vendor, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?){}
        "#,
        engine.returning_id()
    );
    let query = sqlx::query(&sql)
        .bind(&input.name)
        .bind(&input.type_)
        .bind(&key_enc)
        .bind(input.base_url.trim_end_matches('/'))
        .bind(&input.group_name)
        .bind(input.priority)
        .bind(input.weight.max(1))
        .bind(&input.status)
        .bind(&input.relay_mode)
        .bind(&input.upstream_protocol)
        .bind(&input.auth_style)
        .bind(i64::from(input.inject_stream_usage))
        .bind(default_dialect(&input))
        .bind(input.vendor.trim())
        .bind(&now)
        .bind(&now);
    let id = db::insert_id(engine, query, &mut *tx).await?;
    write_models(&mut tx, engine, id, &input.group_name, &rows).await?;
    tx.commit().await?;
    get_by_id(pool, id).await
}

pub async fn update_channel(
    pool: &DbPool,
    enc_key: &[u8; 32],
    id: i64,
    input: ChannelInput,
) -> AppResult<Channel> {
    let rows = input.validate()?;
    let existing = get_by_id(pool, id).await?;
    let key_enc = if input.key.is_empty() {
        existing.key_enc
    } else {
        crypto::encrypt_secret(enc_key, &input.key).map_err(AppError::Other)?
    };

    let engine = db::engine(pool);
    let mut tx = db::begin_write(pool).await?;
    sqlx::query(
        r#"
        UPDATE channels SET
            name = ?, type = ?, key_enc = ?, base_url = ?,
            group_name = ?, priority = ?, weight = ?, status = ?,
            relay_mode = ?, upstream_protocol = ?, auth_style = ?,
            inject_stream_usage = ?, default_dialect = ?, vendor = ?,
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(&input.name)
    .bind(&input.type_)
    .bind(&key_enc)
    .bind(input.base_url.trim_end_matches('/'))
    .bind(&input.group_name)
    .bind(input.priority)
    .bind(input.weight.max(1))
    .bind(&input.status)
    .bind(&input.relay_mode)
    .bind(&input.upstream_protocol)
    .bind(&input.auth_style)
    .bind(i64::from(input.inject_stream_usage))
    .bind(default_dialect(&input))
    .bind(input.vendor.trim())
    .bind(db::now())
    .bind(id)
    .execute(&mut *tx)
    .await?;

    write_models(&mut tx, engine, id, &input.group_name, &rows).await?;
    tx.commit().await?;
    get_by_id(pool, id).await
}

fn default_dialect(input: &ChannelInput) -> &str {
    match input.default_dialect.trim() {
        "" => dialect::STANDARD,
        d => d,
    }
}

pub async fn set_status(pool: &DbPool, id: i64, status: &str) -> AppResult<Channel> {
    sqlx::query("UPDATE channels SET status = ?, updated_at = ? WHERE id = ?")
        .bind(status)
        .bind(db::now())
        .bind(id)
        .execute(pool)
        .await?;
    get_by_id(pool, id).await
}

pub async fn delete_channel(pool: &DbPool, id: i64) -> AppResult<()> {
    let mut tx = db::begin_write(pool).await?;
    for table in ["abilities", "channel_models"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE channel_id = ?"))
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("DELETE FROM channels WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
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

pub async fn list_models(pool: &DbPool, channel_id: i64) -> AppResult<Vec<ChannelModel>> {
    Ok(
        sqlx::query_as("SELECT * FROM channel_models WHERE channel_id = ? ORDER BY id")
            .bind(channel_id)
            .fetch_all(pool)
            .await?,
    )
}

pub async fn get_view(pool: &DbPool, id: i64) -> AppResult<ChannelView> {
    let channel = get_by_id(pool, id).await?;
    let models = list_models(pool, id).await?;
    Ok(ChannelView { channel, models })
}

pub async fn list_views(pool: &DbPool) -> AppResult<Vec<ChannelView>> {
    let channels = list_channels(pool).await?;
    let rows: Vec<ChannelModel> = sqlx::query_as("SELECT * FROM channel_models ORDER BY id")
        .fetch_all(pool)
        .await?;
    Ok(channels
        .into_iter()
        .map(|channel| {
            let models = rows
                .iter()
                .filter(|r| r.channel_id == channel.id)
                .cloned()
                .collect();
            ChannelView { channel, models }
        })
        .collect())
}

pub async fn decrypt_channel_key(enc_key: &[u8; 32], channel: &Channel) -> AppResult<String> {
    crypto::decrypt_secret(enc_key, &channel.key_enc).map_err(AppError::Other)
}

/// Replaces the channel's model rows and rebuilds its abilities from them.
async fn write_models(
    tx: &mut DbTx,
    engine: &dyn SqlEngine,
    channel_id: i64,
    group_name: &str,
    rows: &[ChannelModelInput],
) -> AppResult<()> {
    for table in ["abilities", "channel_models"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE channel_id = ?"))
            .bind(channel_id)
            .execute(&mut **tx)
            .await?;
    }
    let insert_ability = format!(
        "{} INTO abilities (channel_id, group_name, model) VALUES (?, ?, ?)",
        engine.insert_ignore()
    );
    for row in rows {
        sqlx::query(
            "INSERT INTO channel_models (channel_id, model, upstream_model, upstream_dialect) VALUES (?, ?, ?, ?)",
        )
        .bind(channel_id)
        .bind(&row.model)
        .bind(&row.upstream_model)
        .bind(&row.upstream_dialect)
        .execute(&mut **tx)
        .await?;
        sqlx::query(&insert_ability)
            .bind(channel_id)
            .bind(group_name)
            .bind(&row.model)
            .execute(&mut **tx)
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
