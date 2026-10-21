use anyhow::{anyhow, Context, Result};
use std::env;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub encryption_key: [u8; 32],
    pub admin_username: String,
    pub admin_password: String,
    pub default_user_quota: i64,
    pub relay_max_retries: u32,
    /// In-flight requests one account may hold on this process.
    pub relay_user_concurrency: u32,
    /// In-flight verified requests this process accepts in total.
    pub relay_instance_concurrency: u32,
    /// Sustained verified requests per second for one account on this process.
    pub relay_user_qps: u32,
    /// Sustained verified requests per second for this process.
    pub relay_instance_qps: u32,
    /// In-flight legacy (unsigned) keys this process will look up, in total.
    pub relay_legacy_concurrency: u32,
    /// Sustained legacy-key lookups per second on this process.
    pub relay_legacy_qps: u32,
    pub session_ttl_hours: i64,
    /// Days to keep raw `logs` rows before rolling them into `log_daily`. `0` disables archiving.
    pub log_retention_days: i64,
    pub cors_origins: Vec<String>,
    pub health: HealthConfig,
    /// When set, `connect` fills an empty database with sample channels, users, and logs.
    pub seed_demo: bool,
    /// Relay base shown in the user portal. Empty means the local default.
    pub public_relay_url: String,
}

/// Per-process channel health: circuit breaking on consecutive failures and weight reduction
/// on 429.
#[derive(Clone, Debug)]
pub struct HealthConfig {
    pub enabled: bool,
    /// Consecutive failures (connect error, 5xx, broken stream) that eject a channel.
    pub failure_threshold: u32,
    /// First ejection length; doubles on each failed probe up to `max_eject_secs`.
    pub base_eject_secs: u64,
    pub max_eject_secs: u64,
    /// Lowest weight factor a 429 can push a channel to.
    pub weight_floor: f64,
    /// Seconds for a weight factor to climb from 0 back to 1.
    pub recover_secs: u64,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            failure_threshold: 5,
            base_eject_secs: 30,
            max_eject_secs: 600,
            weight_floor: 0.05,
            recover_secs: 60,
        }
    }
}

impl HealthConfig {
    fn from_env() -> Result<Self> {
        let d = Self::default();
        Ok(Self {
            enabled: env_or("CB_ENABLED", d.enabled)?,
            failure_threshold: env_or("CB_FAILURE_THRESHOLD", d.failure_threshold)?.max(1),
            base_eject_secs: env_or("CB_BASE_EJECT_SECS", d.base_eject_secs)?,
            max_eject_secs: env_or("CB_MAX_EJECT_SECS", d.max_eject_secs)?,
            weight_floor: env_or("RL_WEIGHT_FLOOR", d.weight_floor)?.clamp(0.0, 1.0),
            recover_secs: env_or("RL_RECOVER_SECS", d.recover_secs)?,
        })
    }
}

fn env_or<T: std::str::FromStr>(name: &str, default: T) -> Result<T> {
    match env::var(name) {
        Ok(v) => v.trim().parse().map_err(|_| anyhow!("invalid {name}")),
        Err(_) => Ok(default),
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Self::from_env_with_default_port(3000)
    }

    /// Load config; `PORT` env wins, otherwise `default_port` is used.
    pub fn from_env_with_default_port(default_port: u16) -> Result<Self> {
        let _ = dotenvy::dotenv();

        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            let path = PathBuf::from("data/danmo.db");
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            format!("sqlite://{}?mode=rwc", path.display())
        });

        let key_hex = env::var("ENCRYPTION_KEY").unwrap_or_else(|_| {
            // Dev default — must be overridden in production.
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into()
        });
        let encryption_key = parse_encryption_key(&key_hex)?;

        let cors_origins = env::var("CORS_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:5173,http://localhost:5174".into())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(Self {
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into()),
            port: env::var("PORT")
                .unwrap_or_else(|_| default_port.to_string())
                .parse()
                .context("invalid PORT")?,
            database_url,
            encryption_key,
            admin_username: env::var("ADMIN_USERNAME").unwrap_or_else(|_| "admin".into()),
            admin_password: env::var("ADMIN_PASSWORD").unwrap_or_else(|_| "admin123".into()),
            default_user_quota: env::var("DEFAULT_USER_QUOTA")
                .unwrap_or_else(|_| "500000".into())
                .parse()
                .context("invalid DEFAULT_USER_QUOTA")?,
            relay_max_retries: env::var("RELAY_MAX_RETRIES")
                .unwrap_or_else(|_| "2".into())
                .parse()
                .context("invalid RELAY_MAX_RETRIES")?,
            relay_user_concurrency: env_u32("RELAY_USER_CONCURRENCY", 4)?,
            relay_instance_concurrency: env_u32("RELAY_INSTANCE_CONCURRENCY", 32)?,
            relay_user_qps: env_u32("RELAY_USER_QPS", 5)?,
            relay_instance_qps: env_u32("RELAY_INSTANCE_QPS", 50)?,
            relay_legacy_concurrency: env_u32("RELAY_LEGACY_CONCURRENCY", 4)?,
            relay_legacy_qps: env_u32("RELAY_LEGACY_QPS", 10)?,
            session_ttl_hours: env::var("SESSION_TTL_HOURS")
                .unwrap_or_else(|_| "168".into())
                .parse()
                .context("invalid SESSION_TTL_HOURS")?,
            log_retention_days: env::var("LOG_RETENTION_DAYS")
                .unwrap_or_else(|_| "30".into())
                .parse()
                .context("invalid LOG_RETENTION_DAYS")?,
            cors_origins,
            health: HealthConfig::from_env()?,
            seed_demo: env_flag("SEED_DEMO"),
            public_relay_url: env::var("PUBLIC_RELAY_URL").unwrap_or_default(),
        })
    }
}

fn env_u32(name: &str, default: u32) -> Result<u32> {
    match env::var(name) {
        Err(_) => Ok(default),
        Ok(value) if value.is_empty() => Ok(default),
        Ok(value) => value.parse().with_context(|| format!("invalid {name}")),
    }
}

fn env_flag(name: &str) -> bool {
    matches!(
        env::var(name).ok().as_deref().map(str::trim),
        Some("1" | "true" | "yes" | "on")
    )
}

fn parse_encryption_key(hex_key: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(hex_key).context("ENCRYPTION_KEY must be hex")?;
    if bytes.len() != 32 {
        return Err(anyhow!("ENCRYPTION_KEY must be 32 bytes (64 hex chars)"));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
}
