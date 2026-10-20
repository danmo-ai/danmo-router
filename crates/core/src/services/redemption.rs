use crate::db::{self, DbPool};
use crate::error::{AppError, AppResult};
use crate::models::{Redemption, User};
use rand::rngs::OsRng;
use rand::RngCore;

pub async fn create_batch(
    pool: &DbPool,
    name: &str,
    quota: i64,
    count: i64,
) -> AppResult<Vec<Redemption>> {
    if quota <= 0 {
        return Err(AppError::BadRequest("quota must be positive".into()));
    }
    if !(1..=100).contains(&count) {
        return Err(AppError::BadRequest(
            "count must be between 1 and 100".into(),
        ));
    }
    let name = name.trim();
    let mut tx = db::begin_write(pool).await?;
    let mut created = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let id = insert_one(&mut tx, name, quota).await?;
        let row: Redemption = sqlx::query_as("SELECT * FROM redemptions WHERE id = ?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        created.push(row);
    }
    tx.commit().await?;
    Ok(created)
}

async fn insert_one(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    name: &str,
    quota: i64,
) -> AppResult<i64> {
    for _ in 0..5 {
        let code = generate_code();
        let result = sqlx::query(
            "INSERT INTO redemptions (name, code, quota, status) VALUES (?, ?, ?, 'unused')",
        )
        .bind(name)
        .bind(&code)
        .bind(quota)
        .execute(&mut **tx)
        .await;
        match result {
            Ok(r) => return Ok(r.last_insert_rowid()),
            Err(sqlx::Error::Database(e)) if e.message().contains("UNIQUE") => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(AppError::Other(anyhow::anyhow!(
        "failed to allocate a redemption code"
    )))
}

pub async fn list(pool: &DbPool) -> AppResult<Vec<Redemption>> {
    Ok(
        sqlx::query_as("SELECT * FROM redemptions ORDER BY id DESC LIMIT 500")
            .fetch_all(pool)
            .await?,
    )
}

pub async fn delete_unused(pool: &DbPool, id: i64) -> AppResult<()> {
    let status: Option<(String,)> = sqlx::query_as("SELECT status FROM redemptions WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    let status = status.ok_or_else(|| AppError::NotFound("redemption not found".into()))?;
    if status.0 != "unused" {
        return Err(AppError::Conflict("redemption already used".into()));
    }
    let deleted = sqlx::query("DELETE FROM redemptions WHERE id = ? AND status = 'unused'")
        .bind(id)
        .execute(pool)
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(AppError::Conflict("redemption already used".into()));
    }
    Ok(())
}

/// Marks an unused code as used and adds its quota to the user. A second call fails.
pub async fn redeem(pool: &DbPool, user_id: i64, code: &str) -> AppResult<User> {
    let code = code.trim();
    if code.is_empty() {
        return Err(AppError::BadRequest("code is required".into()));
    }
    let mut tx = db::begin_write(pool).await?;
    let updated = sqlx::query(
        r#"
        UPDATE redemptions
        SET status = 'used', redeemed_by = ?, redeemed_at = datetime('now')
        WHERE code = ? AND status = 'unused'
        "#,
    )
    .bind(user_id)
    .bind(code)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT status FROM redemptions WHERE code = ?")
                .bind(code)
                .fetch_optional(&mut *tx)
                .await?;
        return match existing {
            None => Err(AppError::NotFound("invalid redemption code".into())),
            Some(_) => Err(AppError::Conflict("redemption code already used".into())),
        };
    }
    let (quota,): (i64,) = sqlx::query_as("SELECT quota FROM redemptions WHERE code = ?")
        .bind(code)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("UPDATE users SET quota = quota + ?, updated_at = datetime('now') WHERE id = ?")
        .bind(quota)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    crate::services::user::get_by_id(pool, user_id).await
}

fn generate_code() -> String {
    let mut bytes = [0u8; 8];
    OsRng.fill_bytes(&mut bytes);
    format!("rdm-{}", hex::encode(bytes))
}
