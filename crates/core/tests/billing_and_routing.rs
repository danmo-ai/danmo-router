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
    let created = token_svc::create_token(&pool, user.id, "t1", -1, true, None, "", None)
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
            models: "gpt-4o-mini".into(),
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
    let (pool, _config, _dir) = setup().await;
    let user = user_svc::create_user(&pool, "bob", "secret1", "user", 1, "default")
        .await
        .unwrap();
    let created = token_svc::create_token(&pool, user.id, "t1", -1, true, None, "", None)
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
        models: "m1".into(),
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
    input.native_usage = "deepseek_chat".into();
    input.periods = r#"[{"start":"16:30","end":"00:30","multiplier":0.5}]"#.into();
    let saved = billing::upsert_model_price(&pool, &input).await.unwrap();
    assert_eq!(saved.native_usage, "deepseek_chat");
    assert_eq!(saved.cache_read_ratio, 0.1);

    input.native_usage = "nope".into();
    assert!(billing::upsert_model_price(&pool, &input).await.is_err());
    input.native_usage = String::new();
    input.tiers = "not json".into();
    assert!(billing::upsert_model_price(&pool, &input).await.is_err());
}
