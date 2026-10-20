//! Sample channels, users, tokens, and call logs for local portals.
//!
//! Loaded once when `SEED_DEMO` is set. The `demo` user is the lock: the process that creates
//! it fills the rest, and a later boot sees that user and leaves the database alone.

use crate::billing::{self, ModelPriceInput};
use crate::crypto;
use crate::db::DbPool;
use crate::error::{AppError, AppResult};
use crate::models::{Channel, User};
use crate::services::channel::{self, ChannelInput, ChannelModelInput};
use crate::services::log::{self, NewLog};
use crate::services::user;
use tracing::info;

const DEMO_USER: &str = "demo";
const DEMO_PASSWORD: &str = "demo1234";
pub const DEMO_API_KEY: &str = "sk-demo-playground";

pub async fn seed_demo(pool: &DbPool, enc_key: &[u8; 32]) -> AppResult<()> {
    let demo =
        match user::create_user(pool, DEMO_USER, DEMO_PASSWORD, "user", 2_000_000, "vip").await {
            Ok(user) => user,
            Err(AppError::Conflict(_)) => return Ok(()),
            Err(e) => return Err(e),
        };
    let alice = user::create_user(pool, "alice", "alice123", "user", 800_000, "default").await?;
    let bob = user::create_user(pool, "bob", "bob12345", "user", 100_000, "default").await?;
    user::update_user(pool, bob.id, None, None, Some("disabled"), None).await?;

    billing::upsert_group_ratio(pool, "default", 1.0).await?;
    billing::upsert_group_ratio(pool, "vip", 0.8).await?;
    seed_prices(pool).await?;

    let openai = seed_channels(pool, enc_key).await?;
    let playground = insert_token(
        pool,
        demo.id,
        "playground",
        DEMO_API_KEY,
        -1,
        true,
        "active",
    )
    .await?;
    let batch = insert_token(
        pool,
        demo.id,
        "batch",
        "sk-demo-batch",
        50_000,
        false,
        "active",
    )
    .await?;
    insert_token(
        pool,
        demo.id,
        "legacy",
        "sk-demo-legacy",
        -1,
        true,
        "disabled",
    )
    .await?;
    insert_token(
        pool,
        alice.id,
        "notebook",
        "sk-demo-alice",
        -1,
        true,
        "active",
    )
    .await?;

    seed_logs(pool, &demo, alice.id, playground, batch, &openai).await?;
    sqlx::query(
        r#"
        UPDATE users SET used_quota = (
            SELECT COALESCE(SUM(quota), 0) FROM logs
            WHERE logs.user_id = users.id AND logs.status = 'success'
        )
        "#,
    )
    .execute(pool)
    .await?;

    info!("seeded demo data for user '{DEMO_USER}'");
    Ok(())
}

async fn seed_prices(pool: &DbPool) -> AppResult<()> {
    let mut rows = vec![
        price("gpt-4o-mini", 0.15, 0.6, "{}"),
        price("gpt-4o", 2.5, 1.0, "{}"),
        price("text-embedding-3-small", 0.02, 1.0, "{}"),
        price(
            "deepseek-chat",
            0.27,
            1.5,
            r#"{"openai_chat":"thinking","anthropic_messages":"standard"}"#,
        ),
        price(
            "qwen-plus",
            0.8,
            3.0,
            r#"{"openai_chat":"enable_thinking","openai_responses":"thinking"}"#,
        ),
        price("claude-sonnet-4-5", 3.0, 5.0, "{}"),
        price("gemini-2.5-flash", 0.15, 2.5, "{}"),
    ];
    rows[3].cache_read_ratio = 0.1;
    rows[3].cache_write_ratio = 0.5;
    rows[3].tiers = r#"[{"max_input":32000,"model_ratio":0.14},{"model_ratio":0.27}]"#.into();
    rows[4].periods = r#"[{"start":"16:30","end":"00:30","multiplier":0.5}]"#.into();
    for row in &rows {
        billing::upsert_model_price(pool, row).await?;
    }
    Ok(())
}

fn price(model: &str, model_ratio: f64, completion_ratio: f64, native: &str) -> ModelPriceInput {
    let mut input = ModelPriceInput::new(model, model_ratio, completion_ratio);
    input.native_dialects = native.to_string();
    input
}

async fn seed_channels(pool: &DbPool, enc_key: &[u8; 32]) -> AppResult<SeedChannels> {
    let openai = channel(
        pool,
        enc_key,
        "OpenAI · Chat",
        "openai",
        "openai",
        "https://api.openai.com/v1",
        "default",
        10,
        2,
        "enabled",
        "passthrough",
        "openai_chat",
        "bearer",
        "standard",
        vec![
            ChannelModelInput::new("gpt-4o-mini"),
            ChannelModelInput::new("gpt-4o"),
        ],
    )
    .await?;
    let anthropic = channel(
        pool,
        enc_key,
        "Anthropic · Messages",
        "anthropic",
        "anthropic",
        "https://api.anthropic.com",
        "default",
        8,
        1,
        "enabled",
        "passthrough",
        "anthropic_messages",
        "x_api_key",
        "standard",
        vec![ChannelModelInput::new("claude-sonnet-4-5")],
    )
    .await?;
    let deepseek = channel(
        pool,
        enc_key,
        "DeepSeek · Chat",
        "deepseek",
        "deepseek",
        "https://api.deepseek.com",
        "vip",
        20,
        1,
        "enabled",
        "passthrough",
        "openai_chat",
        "bearer",
        "thinking",
        vec![
            ChannelModelInput::new("deepseek-chat"),
            ChannelModelInput::new("deepseek-reasoner"),
        ],
    )
    .await?;
    let qwen = channel(
        pool,
        enc_key,
        "Qwen · 协议转换",
        "dashscope",
        "dashscope",
        "https://dashscope.aliyuncs.com/compatible-mode/v1",
        "default",
        5,
        1,
        "enabled",
        "convert",
        "openai_chat",
        "bearer",
        "enable_thinking",
        vec![
            ChannelModelInput::new("qwen-plus"),
            ChannelModelInput::with("qwen3-max", "qwen3-max-2025", "enable_thinking"),
        ],
    )
    .await?;
    let gemini = channel(
        pool,
        enc_key,
        "Google Gemini",
        "gemini",
        "gemini",
        "https://generativelanguage.googleapis.com/v1beta",
        "default",
        3,
        1,
        "enabled",
        "passthrough",
        "gemini",
        "x_goog_api_key",
        "standard",
        vec![ChannelModelInput::new("gemini-2.5-flash")],
    )
    .await?;
    channel(
        pool,
        enc_key,
        "硅基流动 · 聚合",
        "siliconflow",
        "",
        "https://api.siliconflow.cn/v1",
        "default",
        0,
        1,
        "disabled",
        "passthrough",
        "openai_chat",
        "bearer",
        "opaque",
        vec![ChannelModelInput::with(
            "deepseek-chat",
            "deepseek-ai/DeepSeek-V3",
            "opaque",
        )],
    )
    .await?;

    Ok(SeedChannels {
        openai: openai.id,
        anthropic: anthropic.id,
        deepseek: deepseek.id,
        qwen: qwen.id,
        gemini: gemini.id,
    })
}

#[allow(clippy::too_many_arguments)]
async fn channel(
    pool: &DbPool,
    enc_key: &[u8; 32],
    name: &str,
    type_: &str,
    vendor: &str,
    base_url: &str,
    group_name: &str,
    priority: i64,
    weight: i64,
    status: &str,
    relay_mode: &str,
    upstream_protocol: &str,
    auth_style: &str,
    default_dialect: &str,
    models: Vec<ChannelModelInput>,
) -> AppResult<Channel> {
    channel::create_channel(
        pool,
        enc_key,
        ChannelInput {
            name: name.into(),
            type_: type_.into(),
            key: format!("sk-demo-{type_}-not-a-real-key"),
            base_url: base_url.into(),
            models,
            group_name: group_name.into(),
            priority,
            weight,
            status: status.into(),
            relay_mode: relay_mode.into(),
            upstream_protocol: upstream_protocol.into(),
            auth_style: auth_style.into(),
            inject_stream_usage: true,
            default_dialect: default_dialect.into(),
            vendor: vendor.into(),
        },
    )
    .await
}

struct SeedChannels {
    openai: i64,
    anthropic: i64,
    deepseek: i64,
    qwen: i64,
    gemini: i64,
}

async fn insert_token(
    pool: &DbPool,
    user_id: i64,
    name: &str,
    raw_key: &str,
    remain_quota: i64,
    unlimited: bool,
    status: &str,
) -> AppResult<i64> {
    let key_hash = crypto::hash_token_key(raw_key);
    let key_prefix: String = raw_key.chars().take(10).collect();
    let result = sqlx::query(
        r#"
        INSERT INTO tokens
            (user_id, name, key_hash, key_prefix, status, remain_quota, unlimited_quota, models)
        VALUES (?, ?, ?, ?, ?, ?, ?, '')
        "#,
    )
    .bind(user_id)
    .bind(name)
    .bind(&key_hash)
    .bind(&key_prefix)
    .bind(status)
    .bind(remain_quota)
    .bind(i64::from(unlimited))
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

async fn seed_logs(
    pool: &DbPool,
    demo: &User,
    alice_id: i64,
    playground: i64,
    batch: i64,
    channels: &SeedChannels,
) -> AppResult<()> {
    let rows = [
        DemoLog::ok(
            demo.id,
            playground,
            channels.openai,
            "gpt-4o-mini",
            128,
            86,
            42,
            640,
            "openai_chat",
            "-15 minutes",
        ),
        DemoLog::ok(
            demo.id,
            playground,
            channels.deepseek,
            "deepseek-chat",
            860,
            420,
            196,
            1820,
            "deepseek_chat",
            "-70 minutes",
        )
        .cached(2400, 180, 260),
        DemoLog::ok(
            demo.id,
            batch,
            channels.anthropic,
            "claude-sonnet-4-5",
            2100,
            640,
            1540,
            2400,
            "anthropic_messages",
            "-3 hours",
        ),
        DemoLog::fail(
            demo.id,
            playground,
            channels.qwen,
            "qwen-plus",
            "billing_failed",
            "upstream usage missing",
            410,
            "-5 hours",
        ),
        DemoLog::fail(
            demo.id,
            playground,
            channels.gemini,
            "gemini-2.5-flash",
            "error",
            "upstream 502",
            1200,
            "-8 hours",
        ),
        DemoLog::ok(
            alice_id,
            0,
            channels.openai,
            "gpt-4o",
            300,
            120,
            510,
            980,
            "openai_chat",
            "-1 day",
        )
        .anonymous(),
        DemoLog::ok(
            alice_id,
            0,
            channels.openai,
            "text-embedding-3-small",
            512,
            0,
            6,
            90,
            "embeddings",
            "-2 days",
        )
        .anonymous(),
    ];

    for row in rows {
        let (entry, when) = row.into_entry();
        let saved = log::insert_log(pool, entry).await?;
        sqlx::query("UPDATE logs SET created_at = datetime('now', ?) WHERE id = ?")
            .bind(when)
            .bind(saved.id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

struct DemoLog {
    user_id: i64,
    token_id: Option<i64>,
    channel_id: i64,
    model: &'static str,
    prompt: i64,
    completion: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
    quota: i64,
    duration_ms: i64,
    status: &'static str,
    error: Option<&'static str>,
    adapter: &'static str,
    when: &'static str,
}

impl DemoLog {
    #[allow(clippy::too_many_arguments)]
    fn ok(
        user_id: i64,
        token_id: i64,
        channel_id: i64,
        model: &'static str,
        prompt: i64,
        completion: i64,
        quota: i64,
        duration_ms: i64,
        adapter: &'static str,
        when: &'static str,
    ) -> Self {
        Self {
            user_id,
            token_id: Some(token_id),
            channel_id,
            model,
            prompt,
            completion,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
            quota,
            duration_ms,
            status: "success",
            error: None,
            adapter,
            when,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fail(
        user_id: i64,
        token_id: i64,
        channel_id: i64,
        model: &'static str,
        status: &'static str,
        error: &'static str,
        duration_ms: i64,
        when: &'static str,
    ) -> Self {
        Self {
            user_id,
            token_id: Some(token_id),
            channel_id,
            model,
            prompt: 40,
            completion: 0,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
            quota: 0,
            duration_ms,
            status,
            error: Some(error),
            adapter: "",
            when,
        }
    }

    fn cached(mut self, read: i64, write: i64, reasoning: i64) -> Self {
        self.cache_read = read;
        self.cache_write = write;
        self.reasoning = reasoning;
        self
    }

    fn anonymous(mut self) -> Self {
        self.token_id = None;
        self
    }

    fn into_entry(self) -> (NewLog, &'static str) {
        let mut entry = NewLog::new(self.user_id, self.token_id, self.model);
        entry.channel_id = Some(self.channel_id);
        entry.prompt_tokens = self.prompt;
        entry.completion_tokens = self.completion;
        entry.cache_read_tokens = self.cache_read;
        entry.cache_write_tokens = self.cache_write;
        entry.reasoning_tokens = self.reasoning;
        entry.quota = self.quota;
        entry.duration_ms = self.duration_ms;
        entry.status = self.status.into();
        entry.error = self.error.map(str::to_string);
        entry.usage_adapter = self.adapter.into();
        (entry, self.when)
    }
}
