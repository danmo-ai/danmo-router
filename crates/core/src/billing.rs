use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{GroupRatio, ModelPrice, Token, User};
use serde::{Deserialize, Serialize};
use sqlx::QueryBuilder;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub estimated: bool,
}

#[derive(Debug, Clone)]
pub struct BillingSession {
    pub user_id: i64,
    pub token_id: i64,
    pub group_name: String,
    pub model: String,
    pub pre_consumed: i64,
    pub token_unlimited: bool,
}

pub fn calc_quota(
    prompt_tokens: i64,
    completion_tokens: i64,
    model_ratio: f64,
    completion_ratio: f64,
    group_ratio: f64,
) -> i64 {
    let weighted = prompt_tokens as f64 + completion_tokens as f64 * completion_ratio;
    (weighted * model_ratio * group_ratio).ceil() as i64
}

pub fn estimate_pre_consume(max_tokens: Option<i64>, model_ratio: f64, group_ratio: f64) -> i64 {
    let max_tokens = max_tokens.unwrap_or(1024).clamp(1, 128_000);
    // Conservative: assume prompt ~ max_tokens/2 and completion = max_tokens
    let prompt = max_tokens / 2;
    calc_quota(prompt, max_tokens, model_ratio, 1.0, group_ratio).max(1)
}

pub async fn load_ratios(
    pool: &DbPool,
    model: &str,
    group_name: &str,
) -> AppResult<(f64, f64, f64)> {
    let price: Option<ModelPrice> = sqlx::query_as("SELECT * FROM model_prices WHERE model = ?")
        .bind(model)
        .fetch_optional(pool)
        .await?;
    let (model_ratio, completion_ratio) = price
        .map(|p| (p.model_ratio, p.completion_ratio))
        .unwrap_or((1.0, 1.0));

    let group: Option<GroupRatio> =
        sqlx::query_as("SELECT * FROM group_ratios WHERE group_name = ?")
            .bind(group_name)
            .fetch_optional(pool)
            .await?;
    let group_ratio = group.map(|g| g.ratio).unwrap_or(1.0);
    Ok((model_ratio, completion_ratio, group_ratio))
}

pub async fn pre_consume(
    pool: &DbPool,
    user: &User,
    token: &Token,
    group_name: &str,
    model: &str,
    max_tokens: Option<i64>,
) -> AppResult<BillingSession> {
    let (model_ratio, _completion_ratio, group_ratio) =
        load_ratios(pool, model, group_name).await?;
    let amount = estimate_pre_consume(max_tokens, model_ratio, group_ratio);

    let mut tx = pool.begin().await?;

    let fresh: User = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;
    if fresh.remaining_quota() < amount {
        return Err(AppError::InsufficientQuota(
            "user quota insufficient".into(),
        ));
    }

    let fresh_token: Token = sqlx::query_as("SELECT * FROM tokens WHERE id = ?")
        .bind(token.id)
        .fetch_one(&mut *tx)
        .await?;
    if !fresh_token.is_unlimited() && fresh_token.remain_quota < amount {
        return Err(AppError::InsufficientQuota(
            "token quota insufficient".into(),
        ));
    }

    sqlx::query(
        "UPDATE users SET used_quota = used_quota + ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(amount)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;

    if !fresh_token.is_unlimited() {
        sqlx::query(
            "UPDATE tokens SET remain_quota = remain_quota - ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(amount)
        .bind(token.id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(BillingSession {
        user_id: user.id,
        token_id: token.id,
        group_name: group_name.to_string(),
        model: model.to_string(),
        pre_consumed: amount,
        token_unlimited: fresh_token.is_unlimited(),
    })
}

pub async fn settle(pool: &DbPool, session: &BillingSession, usage: &Usage) -> AppResult<i64> {
    let (model_ratio, completion_ratio, group_ratio) =
        load_ratios(pool, &session.model, &session.group_name).await?;
    let actual = calc_quota(
        usage.prompt_tokens,
        usage.completion_tokens,
        model_ratio,
        completion_ratio,
        group_ratio,
    )
    .max(0);
    let delta = actual - session.pre_consumed;

    let mut tx = pool.begin().await?;
    if delta != 0 {
        sqlx::query(
            "UPDATE users SET used_quota = used_quota + ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(delta)
        .bind(session.user_id)
        .execute(&mut *tx)
        .await?;

        if !session.token_unlimited {
            sqlx::query(
                "UPDATE tokens SET remain_quota = remain_quota - ?, updated_at = datetime('now') WHERE id = ?",
            )
            .bind(delta)
            .bind(session.token_id)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(actual)
}

pub async fn refund(pool: &DbPool, session: &BillingSession) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE users SET used_quota = used_quota - ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(session.pre_consumed)
    .bind(session.user_id)
    .execute(&mut *tx)
    .await?;

    if !session.token_unlimited {
        sqlx::query(
            "UPDATE tokens SET remain_quota = remain_quota + ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(session.pre_consumed)
        .bind(session.token_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn list_model_prices(pool: &DbPool) -> AppResult<Vec<ModelPrice>> {
    Ok(sqlx::query_as("SELECT * FROM model_prices ORDER BY model")
        .fetch_all(pool)
        .await?)
}

pub async fn upsert_model_price(
    pool: &DbPool,
    model: &str,
    model_ratio: f64,
    completion_ratio: f64,
) -> AppResult<ModelPrice> {
    sqlx::query(
        r#"
        INSERT INTO model_prices (model, model_ratio, completion_ratio, updated_at)
        VALUES (?, ?, ?, datetime('now'))
        ON CONFLICT(model) DO UPDATE SET
            model_ratio = excluded.model_ratio,
            completion_ratio = excluded.completion_ratio,
            updated_at = datetime('now')
        "#,
    )
    .bind(model)
    .bind(model_ratio)
    .bind(completion_ratio)
    .execute(pool)
    .await?;

    Ok(sqlx::query_as("SELECT * FROM model_prices WHERE model = ?")
        .bind(model)
        .fetch_one(pool)
        .await?)
}

pub async fn list_group_ratios(pool: &DbPool) -> AppResult<Vec<GroupRatio>> {
    Ok(
        sqlx::query_as("SELECT * FROM group_ratios ORDER BY group_name")
            .fetch_all(pool)
            .await?,
    )
}

pub async fn upsert_group_ratio(
    pool: &DbPool,
    group_name: &str,
    ratio: f64,
) -> AppResult<GroupRatio> {
    sqlx::query(
        r#"
        INSERT INTO group_ratios (group_name, ratio, updated_at)
        VALUES (?, ?, datetime('now'))
        ON CONFLICT(group_name) DO UPDATE SET
            ratio = excluded.ratio,
            updated_at = datetime('now')
        "#,
    )
    .bind(group_name)
    .bind(ratio)
    .execute(pool)
    .await?;

    Ok(
        sqlx::query_as("SELECT * FROM group_ratios WHERE group_name = ?")
            .bind(group_name)
            .fetch_one(pool)
            .await?,
    )
}

/// Estimate usage from text lengths when upstream omits usage.
pub fn estimate_usage_from_chars(prompt_chars: usize, completion_chars: usize) -> Usage {
    Usage {
        prompt_tokens: ((prompt_chars as f64) / 4.0).ceil() as i64,
        completion_tokens: ((completion_chars as f64) / 4.0).ceil() as i64,
        estimated: true,
    }
}

#[allow(dead_code)]
pub fn build_log_filter<'a>(qb: &mut QueryBuilder<'a, sqlx::Sqlite>, user_id: Option<i64>) {
    if let Some(uid) = user_id {
        qb.push(" WHERE user_id = ").push_bind(uid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_formula() {
        // (100 + 50 * 2) * 0.5 * 1.0 = 100
        assert_eq!(calc_quota(100, 50, 0.5, 2.0, 1.0), 100);
        assert_eq!(calc_quota(1, 1, 0.1, 1.0, 1.0), 1);
    }
}
