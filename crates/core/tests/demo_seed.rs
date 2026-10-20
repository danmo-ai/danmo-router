use danmo_core::config::Config;
use danmo_core::crypto::{classify_api_key, ApiKeyClass};
use danmo_core::db::connect;
use danmo_core::demo::{DEMO_ALICE_KEY, DEMO_API_KEY, DEMO_BATCH_KEY, DEMO_DISABLED_KEY};
use danmo_core::services::{
    channel, log as log_svc, redemption, token as token_svc, user as user_svc,
};
mod common;

async fn demo_pool() -> (danmo_core::DbPool, common::TestDb) {
    let db = common::test_db().await;
    let mut config = Config::from_env().unwrap();
    config.database_url = db.url.clone();
    config.seed_demo = true;
    config.admin_username = "admin".into();
    config.admin_password = "admin123".into();
    let pool = connect(&config).await.unwrap();
    (pool, db)
}

#[tokio::test]
async fn demo_seed_fills_portals_and_does_not_repeat() {
    let (pool, db) = demo_pool().await;

    let demo = user_svc::authenticate(&pool, "demo", "demo1234")
        .await
        .unwrap();
    assert_eq!(demo.group_name, "vip");
    assert!(demo.used_quota > 0);
    assert!(demo.remaining_quota() < demo.quota);

    let users = user_svc::list_users(&pool).await.unwrap();
    let bob = users.iter().find(|u| u.username == "bob").unwrap();
    assert_eq!(bob.status, "disabled");

    let channels = channel::list_channels(&pool).await.unwrap();
    assert_eq!(channels.len(), 6);
    assert!(channels.iter().any(|c| c.status == "disabled"));
    assert!(channels.iter().any(|c| c.relay_mode == "convert"));

    let auth = token_svc::authenticate_api_key(&pool, DEMO_API_KEY)
        .await
        .unwrap();
    assert_eq!(auth.user.id, demo.id);
    let enc_key = Config::from_env().unwrap().encryption_key;
    for key in [
        DEMO_API_KEY,
        DEMO_BATCH_KEY,
        DEMO_DISABLED_KEY,
        DEMO_ALICE_KEY,
    ] {
        assert_eq!(
            classify_api_key(&enc_key, key),
            ApiKeyClass::Legacy,
            "{key}"
        );
    }
    let tokens = token_svc::list_by_user(&pool, demo.id).await.unwrap();
    assert_eq!(tokens.len(), 3);
    assert!(tokens.iter().any(|t| t.status == "disabled"));

    let redemptions = redemption::list(&pool).await.unwrap();
    assert_eq!(redemptions.len(), 3);
    assert!(redemptions.iter().all(|r| r.status == "unused"));

    let logs = log_svc::list_logs(
        &pool,
        &log_svc::LogQuery {
            limit: 50,
            ..log_svc::LogQuery::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(logs.len(), 7);
    assert!(logs.iter().any(|l| l.status == "billing_failed"));
    assert!(logs.iter().any(|l| l.reasoning_tokens > 0));

    let mut again = Config::from_env().unwrap();
    again.database_url = db.url.clone();
    again.seed_demo = true;
    let pool2 = connect(&again).await.unwrap();
    assert_eq!(channel::list_channels(&pool2).await.unwrap().len(), 6);
    assert_eq!(
        log_svc::list_logs(
            &pool2,
            &log_svc::LogQuery {
                limit: 50,
                ..log_svc::LogQuery::default()
            },
        )
        .await
        .unwrap()
        .len(),
        7
    );
    assert_eq!(redemption::list(&pool2).await.unwrap().len(), 3);
}
