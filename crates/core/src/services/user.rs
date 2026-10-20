use crate::crypto;
use crate::db::{self, DbPool};
use crate::error::{AppError, AppResult};
use crate::models::User;

pub async fn create_user(
    pool: &DbPool,
    username: &str,
    password: &str,
    role: &str,
    quota: i64,
    group_name: &str,
) -> AppResult<User> {
    if username.trim().is_empty() || password.len() < 6 {
        return Err(AppError::BadRequest(
            "username required and password must be at least 6 characters".into(),
        ));
    }
    let password_hash = crypto::hash_password(password).map_err(AppError::Other)?;
    let now = db::now();
    let engine = db::engine(pool);
    let sql = format!(
        r#"
        INSERT INTO users
            (username, password_hash, role, quota, used_quota, group_name, status, created_at, updated_at)
        VALUES (?, ?, ?, ?, 0, ?, 'active', ?, ?){}
        "#,
        engine.returning_id()
    );
    let query = sqlx::query(&sql)
        .bind(username.trim())
        .bind(password_hash)
        .bind(role)
        .bind(quota)
        .bind(group_name)
        .bind(&now)
        .bind(&now);

    match db::insert_id(engine, query, pool).await {
        Ok(id) => get_by_id(pool, id).await,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            Err(AppError::Conflict("username already exists".into()))
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn authenticate(pool: &DbPool, username: &str, password: &str) -> AppResult<User> {
    let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await?;
    let user = user.ok_or_else(|| AppError::Unauthorized("invalid credentials".into()))?;
    if !user.is_active() {
        return Err(AppError::Forbidden("user disabled".into()));
    }
    let ok = crypto::verify_password(password, &user.password_hash).map_err(AppError::Other)?;
    if !ok {
        return Err(AppError::Unauthorized("invalid credentials".into()));
    }
    Ok(user)
}

pub async fn get_by_id(pool: &DbPool, id: i64) -> AppResult<User> {
    sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("user not found".into()))
}

pub async fn list_users(pool: &DbPool) -> AppResult<Vec<User>> {
    Ok(sqlx::query_as("SELECT * FROM users ORDER BY id DESC")
        .fetch_all(pool)
        .await?)
}

pub async fn update_user(
    pool: &DbPool,
    id: i64,
    quota: Option<i64>,
    group_name: Option<&str>,
    status: Option<&str>,
    role: Option<&str>,
) -> AppResult<User> {
    let mut user = get_by_id(pool, id).await?;
    if let Some(q) = quota {
        user.quota = q;
    }
    if let Some(g) = group_name {
        user.group_name = g.to_string();
    }
    if let Some(s) = status {
        user.status = s.to_string();
    }
    if let Some(r) = role {
        user.role = r.to_string();
    }
    sqlx::query(
        r#"
        UPDATE users
        SET quota = ?, group_name = ?, status = ?, role = ?, updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(user.quota)
    .bind(&user.group_name)
    .bind(&user.status)
    .bind(&user.role)
    .bind(db::now())
    .bind(id)
    .execute(pool)
    .await?;
    get_by_id(pool, id).await
}

pub async fn adjust_quota(pool: &DbPool, id: i64, delta: i64) -> AppResult<User> {
    sqlx::query("UPDATE users SET quota = quota + ?, updated_at = ? WHERE id = ?")
        .bind(delta)
        .bind(db::now())
        .bind(id)
        .execute(pool)
        .await?;
    get_by_id(pool, id).await
}

pub async fn set_password(pool: &DbPool, id: i64, password: &str) -> AppResult<()> {
    if password.len() < 6 {
        return Err(AppError::BadRequest(
            "password must be at least 6 characters".into(),
        ));
    }
    let _ = get_by_id(pool, id).await?;
    let password_hash = crypto::hash_password(password).map_err(AppError::Other)?;
    sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
        .bind(password_hash)
        .bind(db::now())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn change_password(
    pool: &DbPool,
    id: i64,
    old_password: &str,
    new_password: &str,
) -> AppResult<()> {
    let user = get_by_id(pool, id).await?;
    let ok = crypto::verify_password(old_password, &user.password_hash).map_err(AppError::Other)?;
    if !ok {
        return Err(AppError::Unauthorized("invalid password".into()));
    }
    set_password(pool, id, new_password).await
}
