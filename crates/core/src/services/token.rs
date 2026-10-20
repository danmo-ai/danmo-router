use crate::crypto;
use crate::db::{self, DbPool};
use crate::error::{AppError, AppResult};
use crate::models::{Token, User};
use chrono::{DateTime, NaiveDateTime, Utc};

#[derive(Debug, Clone)]
pub struct CreatedToken {
    pub token: Token,
    pub raw_key: String,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedToken {
    pub token: Token,
    pub user: User,
}

#[allow(clippy::too_many_arguments)]
pub async fn create_token(
    pool: &DbPool,
    encryption_key: &[u8; 32],
    user_id: i64,
    name: &str,
    remain_quota: i64,
    unlimited_quota: bool,
    expired_at: Option<&str>,
    models: &str,
    group_name: Option<&str>,
) -> AppResult<CreatedToken> {
    let raw_key = crypto::generate_api_key(encryption_key, user_id);
    let key_hash = crypto::hash_token_key(&raw_key);
    let key_prefix = raw_key.chars().take(10).collect::<String>();
    let expired_at = normalize_optional_expiry(expired_at)?;
    let now = db::now();
    let engine = db::engine(pool);

    let sql = format!(
        r#"
        INSERT INTO tokens
            (user_id, name, key_hash, key_prefix, status, remain_quota, unlimited_quota, expired_at,
             models, group_name, created_at, updated_at)
        VALUES (?, ?, ?, ?, 'active', ?, ?, ?, ?, ?, ?, ?){}
        "#,
        engine.returning_id()
    );
    let query = sqlx::query(&sql)
        .bind(user_id)
        .bind(name)
        .bind(&key_hash)
        .bind(&key_prefix)
        .bind(remain_quota)
        .bind(if unlimited_quota { 1 } else { 0 })
        .bind(expired_at)
        .bind(models)
        .bind(group_name)
        .bind(&now)
        .bind(&now);
    let id = db::insert_id(engine, query, pool).await?;

    let token = get_by_id(pool, id).await?;
    Ok(CreatedToken { token, raw_key })
}

pub async fn get_by_id(pool: &DbPool, id: i64) -> AppResult<Token> {
    sqlx::query_as("SELECT * FROM tokens WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("token not found".into()))
}

pub async fn list_by_user(pool: &DbPool, user_id: i64) -> AppResult<Vec<Token>> {
    Ok(
        sqlx::query_as("SELECT * FROM tokens WHERE user_id = ? ORDER BY id DESC")
            .bind(user_id)
            .fetch_all(pool)
            .await?,
    )
}

pub async fn set_status(pool: &DbPool, id: i64, user_id: i64, status: &str) -> AppResult<Token> {
    let token = get_by_id(pool, id).await?;
    if token.user_id != user_id {
        return Err(AppError::Forbidden("token does not belong to user".into()));
    }
    sqlx::query("UPDATE tokens SET status = ?, updated_at = ? WHERE id = ?")
        .bind(status)
        .bind(db::now())
        .bind(id)
        .execute(pool)
        .await?;
    get_by_id(pool, id).await
}

pub struct TokenUpdate {
    pub name: String,
    pub remain_quota: i64,
    pub unlimited_quota: bool,
    pub expired_at: Option<String>,
    pub models: String,
    pub group_name: Option<String>,
}

pub async fn update_token(
    pool: &DbPool,
    id: i64,
    user_id: i64,
    update: TokenUpdate,
) -> AppResult<Token> {
    let token = get_by_id(pool, id).await?;
    if token.user_id != user_id {
        return Err(AppError::Forbidden("token does not belong to user".into()));
    }
    if update.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    let expired_at = normalize_optional_expiry(update.expired_at.as_deref())?;
    let group_name = update
        .group_name
        .as_deref()
        .map(str::trim)
        .filter(|g| !g.is_empty());
    sqlx::query(
        r#"
        UPDATE tokens
        SET name = ?, remain_quota = ?, unlimited_quota = ?, expired_at = ?, models = ?,
            group_name = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(update.name.trim())
    .bind(update.remain_quota)
    .bind(if update.unlimited_quota { 1 } else { 0 })
    .bind(expired_at)
    .bind(update.models.trim())
    .bind(group_name)
    .bind(db::now())
    .bind(id)
    .execute(pool)
    .await?;
    get_by_id(pool, id).await
}

pub async fn delete_token(pool: &DbPool, id: i64, user_id: i64) -> AppResult<()> {
    let token = get_by_id(pool, id).await?;
    if token.user_id != user_id {
        return Err(AppError::Forbidden("token does not belong to user".into()));
    }
    sqlx::query("DELETE FROM tokens WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn authenticate_api_key(pool: &DbPool, raw_key: &str) -> AppResult<AuthenticatedToken> {
    let key = raw_key.trim();
    let key = key.strip_prefix("Bearer ").unwrap_or(key).trim();
    if key.is_empty() {
        return Err(AppError::Unauthorized("missing api key".into()));
    }
    let key_hash = crypto::hash_token_key(key);
    let token: Option<Token> = sqlx::query_as("SELECT * FROM tokens WHERE key_hash = ?")
        .bind(key_hash)
        .fetch_optional(pool)
        .await?;
    let token = token.ok_or_else(|| AppError::Unauthorized("invalid api key".into()))?;
    if !token.is_active() {
        return Err(AppError::Forbidden("token disabled".into()));
    }
    if token.is_expired() {
        return Err(AppError::Forbidden("token expired".into()));
    }
    let user = crate::services::user::get_by_id(pool, token.user_id).await?;
    if !user.is_active() {
        return Err(AppError::Forbidden("user disabled".into()));
    }
    Ok(AuthenticatedToken { token, user })
}

fn normalize_optional_expiry(raw: Option<&str>) -> AppResult<Option<String>> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Ok(Some(
            dt.with_timezone(&Utc)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        ));
    }
    let normalized = raw.replace('T', " ");
    let with_secs = if normalized.len() == 16 {
        format!("{normalized}:00")
    } else {
        normalized
    };
    NaiveDateTime::parse_from_str(&with_secs, "%Y-%m-%d %H:%M:%S").map_err(|_| {
        AppError::BadRequest("expired_at must be YYYY-MM-DDTHH:MM or RFC3339".into())
    })?;
    Ok(Some(with_secs))
}

pub fn resolve_group(token: &Token, user: &User) -> String {
    token
        .group_name
        .clone()
        .filter(|g| !g.is_empty())
        .unwrap_or_else(|| user.group_name.clone())
}
