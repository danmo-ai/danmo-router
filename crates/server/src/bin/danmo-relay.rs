use anyhow::Result;
use axum::Router;
use danmo_relay::RelayState;
use danmo_server::{init_tracing, load_config, serve};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let (config, pool) = load_config(3000).await?;
    let app = Router::new().merge(danmo_relay::router(RelayState::new(pool, config.clone())));
    serve("danmo-relay", &config, app).await
}
