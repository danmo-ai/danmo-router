use crate::db::{self, DbPool};
use crate::dialect::NativeDialects;
use crate::error::{AppError, AppResult};
use crate::models::{GroupRatio, ModelPrice, Token, User};
use crate::usage::Usage;
use chrono::{NaiveTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use sqlx::QueryBuilder;

#[derive(Debug, Clone)]
pub struct BillingSession {
    pub user_id: i64,
    pub token_id: i64,
    pub group_name: String,
    pub model: String,
    pub pre_consumed: i64,
    pub token_unlimited: bool,
    /// The model's native dialect per protocol; clients are answered in it.
    pub native_dialects: NativeDialects,
}

/// Input-length tier. The first tier whose `max_input` is absent or >= total input applies;
/// unset ratios fall back to the base price.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PriceTier {
    #[serde(default)]
    pub max_input: Option<i64>,
    pub model_ratio: f64,
    #[serde(default)]
    pub completion_ratio: Option<f64>,
    #[serde(default)]
    pub cache_read_ratio: Option<f64>,
    #[serde(default)]
    pub cache_write_ratio: Option<f64>,
}

/// UTC time window multiplying the token price, e.g. off-peak discounts. `end` may wrap past
/// midnight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricePeriod {
    pub start: String,
    pub end: String,
    pub multiplier: f64,
}

impl PricePeriod {
    fn contains(&self, t: NaiveTime) -> bool {
        let (Some(start), Some(end)) = (parse_hhmm(&self.start), parse_hhmm(&self.end)) else {
            return false;
        };
        if start <= end {
            t >= start && t < end
        } else {
            t >= start || t < end
        }
    }
}

fn parse_hhmm(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s.trim(), "%H:%M").ok()
}

/// Ratios relative to the uncached input price: `model_ratio` is quota per input token,
/// the others multiply it.
#[derive(Debug, Clone, PartialEq)]
pub struct PriceRule {
    pub model_ratio: f64,
    pub completion_ratio: f64,
    pub cache_read_ratio: f64,
    pub cache_write_ratio: f64,
    pub search_price: f64,
    pub tiers: Vec<PriceTier>,
    pub periods: Vec<PricePeriod>,
}

impl Default for PriceRule {
    fn default() -> Self {
        Self {
            model_ratio: 1.0,
            completion_ratio: 1.0,
            cache_read_ratio: 1.0,
            cache_write_ratio: 1.0,
            search_price: 0.0,
            tiers: Vec::new(),
            periods: Vec::new(),
        }
    }
}

impl PriceRule {
    pub fn from_price(p: &ModelPrice) -> AppResult<Self> {
        Ok(Self {
            model_ratio: p.model_ratio,
            completion_ratio: p.completion_ratio,
            cache_read_ratio: p.cache_read_ratio,
            cache_write_ratio: p.cache_write_ratio,
            search_price: p.search_price,
            tiers: parse_json_list(&p.tiers, "tiers")?,
            periods: parse_json_list(&p.periods, "periods")?,
        })
    }

    pub fn quota(&self, usage: &Usage, group_ratio: f64, now: NaiveTime) -> i64 {
        let tier = self
            .tiers
            .iter()
            .find(|t| t.max_input.is_none_or(|m| usage.input_tokens <= m));
        let model_ratio = tier.map_or(self.model_ratio, |t| t.model_ratio);
        let completion = tier
            .and_then(|t| t.completion_ratio)
            .unwrap_or(self.completion_ratio);
        let cache_read = tier
            .and_then(|t| t.cache_read_ratio)
            .unwrap_or(self.cache_read_ratio);
        let cache_write = tier
            .and_then(|t| t.cache_write_ratio)
            .unwrap_or(self.cache_write_ratio);
        let period = self
            .periods
            .iter()
            .find(|p| p.contains(now))
            .map_or(1.0, |p| p.multiplier);

        // Reasoning tokens are already part of output_tokens.
        let weighted = usage.uncached_input_tokens() as f64
            + usage.cache_read_tokens as f64 * cache_read
            + usage.cache_write_tokens as f64 * cache_write
            + usage.output_tokens as f64 * completion;
        let total = weighted * model_ratio * period + usage.search_count as f64 * self.search_price;
        (total * group_ratio).ceil().max(0.0) as i64
    }
}

fn parse_json_list<T: for<'de> Deserialize<'de>>(raw: &str, field: &str) -> AppResult<Vec<T>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(raw)
        .map_err(|e| AppError::BadRequest(format!("invalid {field} JSON: {e}")))
}

fn now_utc_time() -> NaiveTime {
    let now = Utc::now();
    NaiveTime::from_hms_opt(now.hour(), now.minute(), 0).unwrap_or_default()
}

pub fn estimate_pre_consume(max_tokens: Option<i64>, model_ratio: f64, group_ratio: f64) -> i64 {
    let max_tokens = max_tokens.unwrap_or(1024).clamp(1, 128_000);
    // Conservative: assume prompt ~ max_tokens/2 and completion = max_tokens
    let prompt = max_tokens / 2;
    let weighted = (prompt + max_tokens) as f64;
    ((weighted * model_ratio * group_ratio).ceil() as i64).max(1)
}

async fn load_price(pool: &DbPool, model: &str) -> AppResult<Option<ModelPrice>> {
    Ok(sqlx::query_as("SELECT * FROM model_prices WHERE model = ?")
        .bind(model)
        .fetch_optional(pool)
        .await?)
}

async fn load_group_ratio(pool: &DbPool, group_name: &str) -> AppResult<f64> {
    let group: Option<GroupRatio> =
        sqlx::query_as("SELECT * FROM group_ratios WHERE group_name = ?")
            .bind(group_name)
            .fetch_optional(pool)
            .await?;
    Ok(group.map(|g| g.ratio).unwrap_or(1.0))
}

pub async fn pre_consume(
    pool: &DbPool,
    user: &User,
    token: &Token,
    group_name: &str,
    model: &str,
    max_tokens: Option<i64>,
) -> AppResult<BillingSession> {
    let price = load_price(pool, model).await?;
    let model_ratio = price.as_ref().map_or(1.0, |p| p.model_ratio);
    let native_dialects = price
        .as_ref()
        .map(|p| NativeDialects::load(&p.native_dialects))
        .unwrap_or_default();
    let group_ratio = load_group_ratio(pool, group_name).await?;
    let amount = estimate_pre_consume(max_tokens, model_ratio, group_ratio);

    let lock = db::engine(pool).lock_rows();
    let mut tx = db::begin_write(pool).await?;

    let fresh: User = sqlx::query_as(&format!("SELECT * FROM users WHERE id = ?{lock}"))
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;
    if fresh.remaining_quota() < amount {
        return Err(AppError::InsufficientQuota(
            "user quota insufficient".into(),
        ));
    }

    let fresh_token: Token = sqlx::query_as(&format!("SELECT * FROM tokens WHERE id = ?{lock}"))
        .bind(token.id)
        .fetch_one(&mut *tx)
        .await?;
    if !fresh_token.is_unlimited() && fresh_token.remain_quota < amount {
        return Err(AppError::InsufficientQuota(
            "token quota insufficient".into(),
        ));
    }

    let now = db::now();
    sqlx::query("UPDATE users SET used_quota = used_quota + ?, updated_at = ? WHERE id = ?")
        .bind(amount)
        .bind(&now)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;

    if !fresh_token.is_unlimited() {
        sqlx::query(
            "UPDATE tokens SET remain_quota = remain_quota - ?, updated_at = ? WHERE id = ?",
        )
        .bind(amount)
        .bind(&now)
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
        native_dialects,
    })
}

pub async fn settle(pool: &DbPool, session: &BillingSession, usage: &Usage) -> AppResult<i64> {
    let rule = match load_price(pool, &session.model).await? {
        Some(p) => PriceRule::from_price(&p)?,
        None => PriceRule::default(),
    };
    let group_ratio = load_group_ratio(pool, &session.group_name).await?;
    let actual = rule.quota(usage, group_ratio, now_utc_time());
    let delta = actual - session.pre_consumed;

    let mut tx = db::begin_write(pool).await?;
    if delta != 0 {
        let now = db::now();
        sqlx::query("UPDATE users SET used_quota = used_quota + ?, updated_at = ? WHERE id = ?")
            .bind(delta)
            .bind(&now)
            .bind(session.user_id)
            .execute(&mut *tx)
            .await?;

        if !session.token_unlimited {
            sqlx::query(
                "UPDATE tokens SET remain_quota = remain_quota - ?, updated_at = ? WHERE id = ?",
            )
            .bind(delta)
            .bind(&now)
            .bind(session.token_id)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(actual)
}

pub async fn refund(pool: &DbPool, session: &BillingSession) -> AppResult<()> {
    let now = db::now();
    let mut tx = db::begin_write(pool).await?;
    sqlx::query("UPDATE users SET used_quota = used_quota - ?, updated_at = ? WHERE id = ?")
        .bind(session.pre_consumed)
        .bind(&now)
        .bind(session.user_id)
        .execute(&mut *tx)
        .await?;

    if !session.token_unlimited {
        sqlx::query(
            "UPDATE tokens SET remain_quota = remain_quota + ?, updated_at = ? WHERE id = ?",
        )
        .bind(session.pre_consumed)
        .bind(&now)
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

#[derive(Debug, Clone)]
pub struct ModelPriceInput {
    pub model: String,
    pub model_ratio: f64,
    pub completion_ratio: f64,
    pub cache_read_ratio: f64,
    pub cache_write_ratio: f64,
    pub search_price: f64,
    pub tiers: String,
    pub periods: String,
    /// JSON object from protocol to dialect name, e.g. `{"openai_chat":"thinking"}`.
    pub native_dialects: String,
}

impl ModelPriceInput {
    pub fn new(model: &str, model_ratio: f64, completion_ratio: f64) -> Self {
        Self {
            model: model.to_string(),
            model_ratio,
            completion_ratio,
            cache_read_ratio: 1.0,
            cache_write_ratio: 1.0,
            search_price: 0.0,
            tiers: String::new(),
            periods: String::new(),
            native_dialects: String::new(),
        }
    }

    fn validate(&self) -> AppResult<()> {
        if self.model.trim().is_empty() {
            return Err(AppError::BadRequest("model is required".into()));
        }
        let ratios = [
            self.model_ratio,
            self.completion_ratio,
            self.cache_read_ratio,
            self.cache_write_ratio,
            self.search_price,
        ];
        if ratios.iter().any(|r| !r.is_finite() || *r < 0.0) {
            return Err(AppError::BadRequest(
                "ratios and prices must be non-negative".into(),
            ));
        }
        parse_json_list::<PriceTier>(&self.tiers, "tiers")?;
        let periods = parse_json_list::<PricePeriod>(&self.periods, "periods")?;
        if periods
            .iter()
            .any(|p| parse_hhmm(&p.start).is_none() || parse_hhmm(&p.end).is_none())
        {
            return Err(AppError::BadRequest(
                "period start/end must be HH:MM".into(),
            ));
        }
        NativeDialects::parse(&self.native_dialects)?;
        Ok(())
    }
}

pub async fn upsert_model_price(pool: &DbPool, input: &ModelPriceInput) -> AppResult<ModelPrice> {
    input.validate()?;
    let upsert = db::engine(pool).upsert(
        "model",
        &[
            "model_ratio",
            "completion_ratio",
            "cache_read_ratio",
            "cache_write_ratio",
            "search_price",
            "tiers",
            "periods",
            "native_dialects",
            "updated_at",
        ],
    );
    sqlx::query(&format!(
        r#"
        INSERT INTO model_prices
            (model, model_ratio, completion_ratio, cache_read_ratio, cache_write_ratio,
             search_price, tiers, periods, native_dialects, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        {upsert}
        "#
    ))
    .bind(&input.model)
    .bind(input.model_ratio)
    .bind(input.completion_ratio)
    .bind(input.cache_read_ratio)
    .bind(input.cache_write_ratio)
    .bind(input.search_price)
    .bind(input.tiers.trim())
    .bind(input.periods.trim())
    .bind(NativeDialects::parse(&input.native_dialects)?.to_json())
    .bind(db::now())
    .execute(pool)
    .await?;

    Ok(sqlx::query_as("SELECT * FROM model_prices WHERE model = ?")
        .bind(&input.model)
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
    let upsert = db::engine(pool).upsert("group_name", &["ratio", "updated_at"]);
    sqlx::query(&format!(
        "INSERT INTO group_ratios (group_name, ratio, updated_at) VALUES (?, ?, ?) {upsert}"
    ))
    .bind(group_name)
    .bind(ratio)
    .bind(db::now())
    .execute(pool)
    .await?;

    Ok(
        sqlx::query_as("SELECT * FROM group_ratios WHERE group_name = ?")
            .bind(group_name)
            .fetch_one(pool)
            .await?,
    )
}

#[allow(dead_code)]
pub fn build_log_filter<'a>(qb: &mut QueryBuilder<'a, sqlx::Any>, user_id: Option<i64>) {
    if let Some(uid) = user_id {
        qb.push(" WHERE user_id = ").push_bind(uid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(input: i64, read: i64, write: i64, output: i64) -> Usage {
        Usage {
            input_tokens: input,
            cache_read_tokens: read,
            cache_write_tokens: write,
            output_tokens: output,
            ..Usage::default()
        }
    }

    fn noon() -> NaiveTime {
        NaiveTime::from_hms_opt(12, 0, 0).unwrap()
    }

    #[test]
    fn base_formula() {
        let rule = PriceRule {
            model_ratio: 0.5,
            completion_ratio: 2.0,
            ..PriceRule::default()
        };
        // (100 + 50 * 2) * 0.5 = 100
        assert_eq!(rule.quota(&usage(100, 0, 0, 50), 1.0, noon()), 100);
        assert_eq!(rule.quota(&usage(100, 0, 0, 50), 2.0, noon()), 200);
    }

    #[test]
    fn cache_ratios_and_reasoning_not_double_counted() {
        let rule = PriceRule {
            model_ratio: 1.0,
            completion_ratio: 4.0,
            cache_read_ratio: 0.1,
            cache_write_ratio: 1.25,
            ..PriceRule::default()
        };
        let mut u = usage(1000, 600, 200, 100);
        u.reasoning_tokens = 80;
        // 200 uncached + 600*0.1 + 200*1.25 + 100*4 = 200 + 60 + 250 + 400
        assert_eq!(rule.quota(&u, 1.0, noon()), 910);
    }

    #[test]
    fn search_fee_tiers_and_periods() {
        let rule = PriceRule {
            model_ratio: 1.0,
            completion_ratio: 1.0,
            search_price: 50.0,
            tiers: vec![
                PriceTier {
                    max_input: Some(32_000),
                    model_ratio: 1.0,
                    ..PriceTier::default()
                },
                PriceTier {
                    max_input: None,
                    model_ratio: 2.0,
                    completion_ratio: Some(3.0),
                    ..PriceTier::default()
                },
            ],
            periods: vec![PricePeriod {
                start: "16:30".into(),
                end: "00:30".into(),
                multiplier: 0.5,
            }],
            ..PriceRule::default()
        };
        let mut small = usage(1000, 0, 0, 100);
        small.search_count = 1;
        assert_eq!(rule.quota(&small, 1.0, noon()), 1100 + 50);
        let large = usage(40_000, 0, 0, 100);
        assert_eq!(rule.quota(&large, 1.0, noon()), (40_000 + 300) * 2);
        let late = NaiveTime::from_hms_opt(23, 0, 0).unwrap();
        assert_eq!(rule.quota(&usage(1000, 0, 0, 100), 1.0, late), 550);
        let early = NaiveTime::from_hms_opt(0, 15, 0).unwrap();
        assert_eq!(rule.quota(&usage(1000, 0, 0, 100), 1.0, early), 550);
    }
}
