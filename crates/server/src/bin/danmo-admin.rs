use anyhow::Result;
use axum::Router;
use danmo_admin_api::AdminState;
use danmo_server::{init_tracing, load_config, serve};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let (config, pool) = load_config(3001).await?;
    let app = Router::new().merge(danmo_admin_api::router(AdminState::new(
        pool,
        config.clone(),
    )));
    serve("danmo-admin", &config, app).await
}
