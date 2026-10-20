use crate::health::ChannelHealth;
use danmo_core::{Config, DbPool};
use reqwest::Client;
use std::sync::Arc;

#[derive(Clone)]
pub struct RelayState {
    pub pool: DbPool,
    pub config: Arc<Config>,
    pub http: Client,
    pub health: Arc<ChannelHealth>,
}

impl RelayState {
    pub fn new(pool: DbPool, config: Arc<Config>) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("http client");
        let health = Arc::new(ChannelHealth::new(config.health.clone()));
        Self {
            pool,
            config,
            http,
            health,
        }
    }
}
