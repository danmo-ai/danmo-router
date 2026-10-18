//! All-in-one process for local development. Prefer split binaries in production.
use anyhow::Result;
use axum::Router;
use danmo_admin_api::AdminState;
use danmo_relay::RelayState;
use danmo_server::{init_tracing, load_config, serve};
use danmo_user_api::UserState;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let (config, pool) = load_config(3000).await?;

    let app = Router::new()
        .merge(danmo_relay::router(RelayState::new(
            pool.clone(),
            config.clone(),
        )))
        .merge(danmo_admin_api::router(AdminState::new(
            pool.clone(),
            config.clone(),
        )))
        .merge(danmo_user_api::router(UserState::new(pool, config.clone())));

    serve("danmo-router", &config, app).await
}
