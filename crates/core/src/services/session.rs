use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{Session, User};
use chrono::{Duration, Utc};
use uuid::Uuid;

pub async fn create_session(pool: &DbPool, user_id: i64, ttl_hours: i64) -> AppResult<Session> {
    let id = Uuid::new_v4().to_string();
    let expires_at = (Utc::now() + Duration::hours(ttl_hours)).to_rfc3339();
    sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(user_id)
        .bind(&expires_at)
        .execute(pool)
        .await?;
    Ok(Session {
        id,
        user_id,
        expires_at,
        created_at: Utc::now().to_rfc3339(),
    })
}

pub async fn get_user_by_session(pool: &DbPool, session_id: &str) -> AppResult<User> {
    let session: Option<Session> = sqlx::query_as("SELECT * FROM sessions WHERE id = ?")
        .bind(session_id)
        .fetch_optional(pool)
        .await?;
    let session = session.ok_or_else(|| AppError::Unauthorized("not logged in".into()))?;

    let expired = chrono::DateTime::parse_from_rfc3339(&session.expires_at)
        .map(|dt| dt.with_timezone(&Utc) < Utc::now())
        .unwrap_or(true);
    if expired {
        let _ = delete_session(pool, session_id).await;
        return Err(AppError::Unauthorized("session expired".into()));
    }

    crate::services::user::get_by_id(pool, session.user_id).await
}

pub async fn delete_session(pool: &DbPool, session_id: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}
