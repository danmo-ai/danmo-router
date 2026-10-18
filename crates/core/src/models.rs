use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: String,
    pub quota: i64,
    pub used_quota: i64,
    pub group_name: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    pub fn is_active(&self) -> bool {
        self.status == "active"
    }

    pub fn remaining_quota(&self) -> i64 {
        self.quota - self.used_quota
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Token {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    #[serde(skip_serializing)]
    pub key_hash: String,
    pub key_prefix: String,
    pub status: String,
    pub remain_quota: i64,
    pub unlimited_quota: i64,
    pub expired_at: Option<String>,
    pub models: String,
    pub group_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Token {
    pub fn is_active(&self) -> bool {
        self.status == "active"
    }

    pub fn is_expired(&self) -> bool {
        let Some(exp) = &self.expired_at else {
            return false;
        };
        DateTime::parse_from_rfc3339(exp)
            .map(|dt| dt.with_timezone(&Utc) < Utc::now())
            .unwrap_or(false)
            || chrono::NaiveDateTime::parse_from_str(exp, "%Y-%m-%d %H:%M:%S")
                .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc) < Utc::now())
                .unwrap_or(false)
    }

    pub fn allows_model(&self, model: &str) -> bool {
        let models = self.models.trim();
        if models.is_empty() {
            return true;
        }
        models
            .split(',')
            .map(str::trim)
            .any(|m| m == model || m == "*")
    }

    pub fn is_unlimited(&self) -> bool {
        self.unlimited_quota != 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Channel {
    pub id: i64,
    pub name: String,
    #[serde(rename = "type")]
    #[sqlx(rename = "type")]
    pub type_: String,
    #[serde(skip_serializing)]
    pub key_enc: String,
    pub base_url: String,
    pub models: String,
    pub group_name: String,
    pub priority: i64,
    pub weight: i64,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl Channel {
    pub fn is_enabled(&self) -> bool {
        self.status == "enabled"
    }

    pub fn model_list(&self) -> Vec<String> {
        self.models
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Ability {
    pub id: i64,
    pub channel_id: i64,
    pub group_name: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ModelPrice {
    pub id: i64,
    pub model: String,
    pub model_ratio: f64,
    pub completion_ratio: f64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GroupRatio {
    pub id: i64,
    pub group_name: String,
    pub ratio: f64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LogEntry {
    pub id: i64,
    pub user_id: i64,
    pub token_id: Option<i64>,
    pub channel_id: Option<i64>,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub quota: i64,
    pub duration_ms: i64,
    pub status: String,
    pub estimated: i64,
    pub error: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Session {
    pub id: String,
    pub user_id: i64,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverviewStats {
    pub total_requests: i64,
    pub success_requests: i64,
    pub total_quota: i64,
    pub total_users: i64,
    pub total_channels: i64,
}
