use danmo_core::billing;
use danmo_core::config::Config;
use danmo_core::crypto;
use danmo_core::db::connect;
use danmo_core::protocol::Protocol;
use danmo_core::routing;
use danmo_core::services::{channel, token as token_svc, user as user_svc};
use danmo_core::usage::Usage;
use std::sync::Arc;
use tempfile::tempdir;

async fn setup() -> (danmo_core::DbPool, Arc<Config>, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let mut config = Config::from_env().unwrap();
    config.database_url = format!("sqlite://{}?mode=rwc", db_path.display());
    config.seed_demo = false;
    config.admin_username = "admin".into();
    config.admin_password = "admin123".into();
    let pool = connect(&config).await.unwrap();
    (pool, Arc::new(config), dir)
}

#[tokio::test]
async fn token_auth_and_billing_settle_refund() {
    let (pool, config, _dir) = setup().await;

    let user = user_svc::create_user(&pool, "alice", "secret1", "user", 10_000, "default")
        .await
        .unwrap();
    let created = token_svc::create_token(
        &pool,
        &config.encryption_key,
        user.id,
        "t1",
        -1,
        true,
        None,
        "",
        None,
    )
    .await
    .unwrap();

    let auth = token_svc::authenticate_api_key(&pool, &created.raw_key)
        .await
        .unwrap();
    assert_eq!(auth.user.id, user.id);

    let session = billing::pre_consume(
        &pool,
        &auth.user,
        &auth.token,
        "default",
        "gpt-4o-mini",
        Some(100),
    )
    .await
    .unwrap();
    assert!(session.pre_consumed > 0);

    let after_pre = user_svc::get_by_id(&pool, user.id).await.unwrap();
    assert_eq!(after_pre.used_quota, session.pre_consumed);

    let actual = billing::settle(
        &pool,
        &session,
        &Usage {
            input_tokens: 10,
            output_tokens: 20,
            ..Usage::default()
        },
    )
    .await
    .unwrap();
    assert!(actual > 0);

    let settled_user = user_svc::get_by_id(&pool, user.id).await.unwrap();
    assert_eq!(settled_user.used_quota, actual);

    // Refund path on a fresh pre-consume
    let session2 = billing::pre_consume(
        &pool,
        &settled_user,
        &auth.token,
        "default",
        "gpt-4o-mini",
        Some(50),
    )
    .await
    .unwrap();
    let before_refund = user_svc::get_by_id(&pool, user.id)
        .await
        .unwrap()
        .used_quota;
    billing::refund(&pool, &session2).await.unwrap();
    let after_refund = user_svc::get_by_id(&pool, user.id)
        .await
        .unwrap()
        .used_quota;
    assert_eq!(after_refund, before_refund - session2.pre_consumed);

    // channel routing
    let _ = channel::create_channel(
        &pool,
        &config.encryption_key,
        channel::ChannelInput {
            name: "c1".into(),
            type_: "openai_compat".into(),
            key: "sk-test".into(),
            base_url: "https://example.com".into(),
            models: channel::ChannelModelInput::list("gpt-4o-mini"),
            group_name: "default".into(),
            priority: 10,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let candidates =
        routing::select_channels(&pool, "default", "gpt-4o-mini", Protocol::OpenaiChat, &[])
            .await
            .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].channel.name, "c1");

    let secret =
        crypto::decrypt_secret(&config.encryption_key, &candidates[0].channel.key_enc).unwrap();
    assert_eq!(secret, "sk-test");
}

#[tokio::test]
async fn insufficient_quota_rejected() {
    let (pool, config, _dir) = setup().await;
    let user = user_svc::create_user(&pool, "bob", "secret1", "user", 1, "default")
        .await
        .unwrap();
    let created = token_svc::create_token(
        &pool,
        &config.encryption_key,
        user.id,
        "t1",
        -1,
        true,
        None,
        "",
        None,
    )
    .await
    .unwrap();
    let auth = token_svc::authenticate_api_key(&pool, &created.raw_key)
        .await
        .unwrap();

    let err = billing::pre_consume(
        &pool,
        &auth.user,
        &auth.token,
        "default",
        "gpt-4o-mini",
        Some(4096),
    )
    .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn routing_filters_by_inbound_protocol() {
    let (pool, config, _dir) = setup().await;
    let make = |name: &str, mode: &str, protocol: &str, priority: i64| channel::ChannelInput {
        name: name.into(),
        key: "sk".into(),
        base_url: "https://example.com".into(),
        models: channel::ChannelModelInput::list("m1"),
        priority,
        relay_mode: mode.into(),
        upstream_protocol: protocol.into(),
        ..Default::default()
    };
    for input in [
        make("chat-pass", "passthrough", "openai_chat", 3),
        make("msg-pass", "passthrough", "anthropic_messages", 2),
        make("msg-convert", "convert", "anthropic_messages", 1),
    ] {
        channel::create_channel(&pool, &config.encryption_key, input)
            .await
            .unwrap();
    }

    let names = |c: Vec<routing::ChannelCandidate>| -> Vec<String> {
        c.into_iter().map(|c| c.channel.name).collect()
    };
    let chat = routing::select_channels(&pool, "default", "m1", Protocol::OpenaiChat, &[])
        .await
        .unwrap();
    assert_eq!(names(chat), vec!["chat-pass", "msg-convert"]);
    let msgs = routing::select_channels(&pool, "default", "m1", Protocol::AnthropicMessages, &[])
        .await
        .unwrap();
    assert_eq!(names(msgs), vec!["msg-pass", "msg-convert"]);
    let gemini = routing::select_channels(&pool, "default", "m1", Protocol::Gemini, &[])
        .await
        .unwrap();
    assert_eq!(names(gemini), vec!["msg-convert"]);
    assert!(
        routing::select_channels(&pool, "default", "m1", Protocol::Embeddings, &[])
            .await
            .is_err()
    );

    let bad = channel::create_channel(
        &pool,
        &config.encryption_key,
        make("emb-convert", "convert", "embeddings", 0),
    )
    .await;
    assert!(bad.is_err());
}

#[tokio::test]
async fn migrations_are_idempotent_and_prices_validate() {
    let (pool, config, _dir) = setup().await;
    drop(pool);
    let pool = connect(&config).await.unwrap();

    let mut input = billing::ModelPriceInput::new("deepseek-chat", 0.5, 2.0);
    input.cache_read_ratio = 0.1;
    input.native_dialects = r#"{"openai_chat":"thinking","anthropic_messages":"standard"}"#.into();
    input.periods = r#"[{"start":"16:30","end":"00:30","multiplier":0.5}]"#.into();
    let saved = billing::upsert_model_price(&pool, &input).await.unwrap();
    assert_eq!(
        saved.native_dialects,
        r#"{"anthropic_messages":"standard","openai_chat":"thinking"}"#
    );
    assert_eq!(saved.cache_read_ratio, 0.1);

    for bad in [
        r#"{"openai_chat":"nope"}"#,
        r#"{"openai_chat":"opaque"}"#,
        r#"{"gemini":"thinking"}"#,
        r#"{"carrier_pigeon":"standard"}"#,
        "[]",
    ] {
        input.native_dialects = bad.into();
        assert!(
            billing::upsert_model_price(&pool, &input).await.is_err(),
            "{bad}"
        );
    }
    input.native_dialects = String::new();
    input.tiers = "not json".into();
    assert!(billing::upsert_model_price(&pool, &input).await.is_err());
}

#[tokio::test]
async fn channel_rows_carry_upstream_model_and_dialect() {
    let (pool, config, _dir) = setup().await;
    let qianfan = channel::create_channel(
        &pool,
        &config.encryption_key,
        channel::ChannelInput {
            name: "qianfan".into(),
            key: "sk".into(),
            base_url: "https://qianfan.example.com/v2".into(),
            default_dialect: "qianfan".into(),
            vendor: "baidu".into(),
            models: vec![
                channel::ChannelModelInput::new("deepseek-v4-flash"),
                channel::ChannelModelInput::with("ernie-5.0", "ernie-5.0-preview", "qianfan_ernie"),
                channel::ChannelModelInput::with("same", "same", ""),
            ],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(qianfan.default_dialect, "qianfan");
    let rows = channel::list_models(&pool, qianfan.id).await.unwrap();
    let got: Vec<_> = rows
        .iter()
        .map(|r| {
            (
                r.model.as_str(),
                r.upstream_model.as_str(),
                r.upstream_dialect.as_str(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            ("deepseek-v4-flash", "", "qianfan"),
            ("ernie-5.0", "ernie-5.0-preview", "qianfan_ernie"),
            ("same", "", "qianfan"),
        ]
    );

    let ernie = routing::select_channels(&pool, "default", "ernie-5.0", Protocol::OpenaiChat, &[])
        .await
        .unwrap();
    assert_eq!(ernie[0].upstream_model(), "ernie-5.0-preview");
    assert_eq!(ernie[0].dialect().id(), "openai_chat/qianfan_ernie");
    let ds = routing::select_channels(
        &pool,
        "default",
        "deepseek-v4-flash",
        Protocol::OpenaiChat,
        &[],
    )
    .await
    .unwrap();
    assert_eq!(ds[0].upstream_model(), "deepseek-v4-flash");
    assert_eq!(ds[0].dialect().name, "qianfan");

    let bad = |models, default: &str| channel::ChannelInput {
        name: "bad".into(),
        key: "sk".into(),
        base_url: "https://example.com".into(),
        default_dialect: default.into(),
        models,
        ..Default::default()
    };
    for input in [
        bad(
            vec![channel::ChannelModelInput::with("m", "", "effort")],
            "standard",
        ),
        bad(
            channel::ChannelModelInput::list("m"),
            "enable_thinking_typo",
        ),
        bad(channel::ChannelModelInput::list("m,m"), "standard"),
    ] {
        assert!(
            channel::create_channel(&pool, &config.encryption_key, input)
                .await
                .is_err()
        );
    }

    channel::update_channel(
        &pool,
        &config.encryption_key,
        qianfan.id,
        channel::ChannelInput {
            name: "qianfan".into(),
            base_url: "https://qianfan.example.com/v2".into(),
            models: channel::ChannelModelInput::list("ernie-5.0"),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(routing::select_channels(
        &pool,
        "default",
        "deepseek-v4-flash",
        Protocol::OpenaiChat,
        &[]
    )
    .await
    .is_err());
    let view = channel::get_view(&pool, qianfan.id).await.unwrap();
    assert_eq!(view.models.len(), 1);
    assert_eq!(view.models[0].upstream_dialect, "standard");

    channel::delete_channel(&pool, qianfan.id).await.unwrap();
    let left: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM channel_models")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left.0, 0);
}

#[tokio::test]
async fn opaque_rows_only_serve_their_own_protocol() {
    let (pool, config, _dir) = setup().await;
    for (name, dialect, priority) in [("router", "opaque", 2), ("vendor", "thinking", 1)] {
        channel::create_channel(
            &pool,
            &config.encryption_key,
            channel::ChannelInput {
                name: name.into(),
                key: "sk".into(),
                base_url: "https://example.com".into(),
                relay_mode: "convert".into(),
                default_dialect: dialect.into(),
                models: channel::ChannelModelInput::list("deepseek-v4-flash"),
                priority,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }
    let names = |p| {
        let pool = pool.clone();
        async move {
            routing::select_channels(&pool, "default", "deepseek-v4-flash", p, &[])
                .await
                .unwrap()
                .into_iter()
                .map(|c| c.channel.name)
                .collect::<Vec<_>>()
        }
    };
    assert_eq!(names(Protocol::OpenaiChat).await, vec!["router", "vendor"]);
    assert_eq!(names(Protocol::AnthropicMessages).await, vec!["vendor"]);
}

/// A database created before channel_models: comma-separated models plus a JSON mapping on the
/// channel, and `native_usage` on the price.
#[tokio::test]
async fn migration_003_moves_models_into_rows() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("old.db");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let old = SqlitePoolOptions::new()
        .connect_with(SqliteConnectOptions::from_str(&url).unwrap())
        .await
        .unwrap();
    for sql in [
        include_str!("../migrations/001_init.sql"),
        include_str!("../migrations/002_dialect.sql"),
        r#"INSERT INTO channels (name, key_enc, base_url, models, model_mapping, upstream_protocol)
           VALUES ('chat', 'k', 'https://a', 'deepseek-chat, gpt-4o ,deepseek-chat,', '{"gpt-4o":"openai/gpt-4o"}', 'openai_chat');
           INSERT INTO channels (name, key_enc, base_url, models, model_mapping, upstream_protocol)
           VALUES ('msgs', 'k', 'https://b', 'deepseek-chat', 'not json', 'anthropic_messages');
           INSERT INTO abilities (channel_id, group_name, model) VALUES (1, 'default', 'deepseek-chat');
           INSERT INTO model_prices (model, native_usage) VALUES ('deepseek-chat', 'deepseek_chat');
           INSERT INTO model_prices (model, native_usage) VALUES ('kimi-k2', 'kimi_chat');
           INSERT INTO model_prices (model, native_usage) VALUES ('gpt-4o', '')"#,
    ] {
        for stmt in sql.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            sqlx::query(stmt).execute(&old).await.unwrap();
        }
    }
    old.close().await;

    let mut config = Config::from_env().unwrap();
    config.database_url = url;
    config.seed_demo = false;
    let pool = connect(&config).await.unwrap();

    let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT channel_id, model, upstream_model, upstream_dialect FROM channel_models ORDER BY channel_id, model",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let rows: Vec<_> = rows
        .iter()
        .map(|(c, m, u, d)| (*c, m.as_str(), u.as_str(), d.as_str()))
        .collect();
    assert_eq!(
        rows,
        vec![
            (1, "deepseek-chat", "", "thinking"),
            (1, "gpt-4o", "openai/gpt-4o", "standard"),
            (2, "deepseek-chat", "", "standard"),
        ]
    );

    let prices: Vec<(String, String)> =
        sqlx::query_as("SELECT model, native_dialects FROM model_prices ORDER BY model")
            .fetch_all(&pool)
            .await
            .unwrap();
    let find = |m: &str| prices.iter().find(|p| p.0 == m).unwrap().1.clone();
    assert_eq!(find("deepseek-chat"), r#"{"openai_chat":"thinking"}"#);
    assert_eq!(find("kimi-k2"), r#"{"openai_chat":"kimi"}"#);
    assert_eq!(find("gpt-4o"), "");

    let ch = channel::get_by_id(&pool, 1).await.unwrap();
    assert_eq!(ch.default_dialect, "standard");
    let cols: Vec<(String,)> = sqlx::query_as("SELECT name FROM pragma_table_info('channels')")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(!cols
        .iter()
        .any(|c| c.0 == "models" || c.0 == "model_mapping"));

    let routed =
        routing::select_channels(&pool, "default", "deepseek-chat", Protocol::OpenaiChat, &[])
            .await
            .unwrap();
    assert_eq!(routed[0].dialect().id(), "openai_chat/thinking");

    drop(pool);
    let pool = connect(&config).await.unwrap();
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM channel_models")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 3);
}
