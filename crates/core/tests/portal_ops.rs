use danmo_core::config::Config;
use danmo_core::db::connect;
use danmo_core::services::{log as log_svc, redemption, user as user_svc};
use danmo_core::AppError;
use sqlx::any::AnyPoolOptions;

mod common;

async fn setup() -> (danmo_core::DbPool, common::TestDb) {
    let db = common::test_db().await;
    let mut config = Config::from_env().unwrap();
    config.database_url = db.url.clone();
    config.seed_demo = false;
    config.admin_username = format!("admin-{}", std::process::id());
    let pool = connect(&config).await.unwrap();
    (pool, db)
}

#[tokio::test]
async fn redeem_adds_quota_once() {
    let (pool, _dir) = setup().await;
    let user = user_svc::create_user(&pool, "alice", "secret1", "user", 100, "default")
        .await
        .unwrap();
    let codes = redemption::create_batch(&pool, "gift", 1_000, 2)
        .await
        .unwrap();
    assert_eq!(codes.len(), 2);
    assert!(codes.iter().all(|c| c.status == "unused"));

    let updated = redemption::redeem(&pool, user.id, &codes[0].code)
        .await
        .unwrap();
    assert_eq!(updated.quota, 1_100);

    let again = redemption::redeem(&pool, user.id, &codes[0].code).await;
    assert!(matches!(again, Err(AppError::Conflict(_))));

    let missing = redemption::redeem(&pool, user.id, "rdm-nope").await;
    assert!(matches!(missing, Err(AppError::NotFound(_))));

    let second = redemption::redeem(&pool, user.id, &codes[1].code)
        .await
        .unwrap();
    assert_eq!(second.quota, 2_100);

    let err = redemption::delete_unused(&pool, codes[0].id).await;
    assert!(matches!(err, Err(AppError::Conflict(_))));
    redemption::delete_unused(
        &pool,
        redemption::create_batch(&pool, "", 10, 1).await.unwrap()[0].id,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn logs_filter_by_model_and_status() {
    let (pool, _dir) = setup().await;
    let user = user_svc::create_user(&pool, "bob", "secret1", "user", 100, "default")
        .await
        .unwrap();

    let mut ok = log_svc::NewLog::new(user.id, None, "gpt-4o");
    ok.status = "success".into();
    ok.quota = 10;
    log_svc::insert_log(&pool, ok).await.unwrap();

    let mut failed = log_svc::NewLog::new(user.id, None, "gpt-4o-mini");
    failed.status = "error".into();
    failed.quota = 0;
    log_svc::insert_log(&pool, failed).await.unwrap();

    let mut other = log_svc::NewLog::new(user.id, None, "gpt-4o");
    other.status = "billing_failed".into();
    log_svc::insert_log(&pool, other).await.unwrap();

    let matched = log_svc::list_logs(
        &pool,
        &log_svc::LogQuery {
            user_id: Some(user.id),
            model: Some("gpt-4o".into()),
            status: Some("success".into()),
            limit: 50,
            ..log_svc::LogQuery::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].model, "gpt-4o");
    assert_eq!(matched[0].status, "success");

    let errors = log_svc::list_logs(
        &pool,
        &log_svc::LogQuery {
            status: Some("error".into()),
            limit: 50,
            ..log_svc::LogQuery::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].model, "gpt-4o-mini");
}

/// relay / admin / user start together against one empty database.
#[tokio::test]
async fn concurrent_boots_apply_each_migration_once() {
    let db = common::test_db().await;
    let mut config = Config::from_env().unwrap();
    config.database_url = db.url.clone();
    config.seed_demo = false;
    let (a, b, c) = tokio::join!(connect(&config), connect(&config), connect(&config));
    let pool = a.unwrap();
    b.unwrap();
    c.unwrap();

    let versions: Vec<(i64,)> =
        sqlx::query_as("SELECT version FROM schema_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    let expected = danmo_core::db::engine(&pool).migrations().len() as i64;
    assert_eq!(versions, (1..=expected).map(|v| (v,)).collect::<Vec<_>>());
    let admins: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE username = ?")
        .bind(&config.admin_username)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(admins.0, 1);
}

/// SQLite: a second `BEGIN IMMEDIATE` cannot take the write lock. MySQL: a second
/// `SELECT ... FOR UPDATE` on the same row times out waiting for its row lock.
#[tokio::test]
async fn held_write_lock_reports_busy() {
    let (pool, db) = setup().await;
    let lock = danmo_core::db::engine(&pool).lock_rows();
    let row = format!("SELECT id FROM users ORDER BY id LIMIT 1{lock}");
    let mut held = danmo_core::db::begin_write(&pool).await.unwrap();
    sqlx::query(&row).fetch_one(&mut *held).await.unwrap();

    let impatient = if db.is_mysql() {
        "SET SESSION innodb_lock_wait_timeout = 1"
    } else {
        "PRAGMA busy_timeout = 0"
    };
    let other = AnyPoolOptions::new()
        .max_connections(1)
        .after_connect(move |conn, _| {
            Box::pin(async move {
                sqlx::query(impatient).execute(conn).await?;
                Ok(())
            })
        })
        .connect(&db.url)
        .await
        .unwrap();
    let err = match danmo_core::db::begin_write(&other).await {
        Err(e) => e,
        Ok(mut tx) => sqlx::query(&row)
            .fetch_one(&mut *tx)
            .await
            .map(|_| ())
            .map_err(AppError::from)
            .unwrap_err(),
    };
    assert!(err.is_db_busy(), "{err}");
    assert!(!AppError::Conflict("x".into()).is_db_busy());
}
