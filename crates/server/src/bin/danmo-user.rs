use anyhow::Result;
use axum::Router;
use danmo_server::{init_tracing, load_config, serve};
use danmo_user_api::UserState;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let (config, pool) = load_config(3002).await?;
    let app = Router::new().merge(danmo_user_api::router(UserState::new(pool, config.clone())));
    serve("danmo-user", &config, app).await
}
