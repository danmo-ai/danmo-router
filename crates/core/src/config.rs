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
    pub cors_origins: Vec<String>,
    /// When set, `connect` fills an empty database with sample channels, users, and logs.
    pub seed_demo: bool,
    /// Relay base shown in the user portal. Empty means the local default.
    pub public_relay_url: String,
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
            cors_origins,
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
