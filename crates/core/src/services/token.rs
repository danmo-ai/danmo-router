use crate::crypto;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{Token, User};

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
    user_id: i64,
    name: &str,
    remain_quota: i64,
    unlimited_quota: bool,
    expired_at: Option<&str>,
    models: &str,
    group_name: Option<&str>,
) -> AppResult<CreatedToken> {
    let raw_key = crypto::generate_api_key();
    let key_hash = crypto::hash_token_key(&raw_key);
    let key_prefix = raw_key.chars().take(10).collect::<String>();

    let result = sqlx::query(
        r#"
        INSERT INTO tokens
            (user_id, name, key_hash, key_prefix, status, remain_quota, unlimited_quota, expired_at, models, group_name)
        VALUES (?, ?, ?, ?, 'active', ?, ?, ?, ?, ?)
        "#,
    )
    .bind(user_id)
    .bind(name)
    .bind(&key_hash)
    .bind(&key_prefix)
    .bind(remain_quota)
    .bind(if unlimited_quota { 1 } else { 0 })
    .bind(expired_at)
    .bind(models)
    .bind(group_name)
    .execute(pool)
    .await?;

    let token = get_by_id(pool, result.last_insert_rowid()).await?;
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
    sqlx::query("UPDATE tokens SET status = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(status)
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

pub fn resolve_group(token: &Token, user: &User) -> String {
    token
        .group_name
        .clone()
        .filter(|g| !g.is_empty())
        .unwrap_or_else(|| user.group_name.clone())
}
