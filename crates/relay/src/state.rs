use danmo_core::{Config, DbPool};
use reqwest::Client;
use std::sync::Arc;

#[derive(Clone)]
pub struct RelayState {
    pub pool: DbPool,
    pub config: Arc<Config>,
    pub http: Client,
}

impl RelayState {
    pub fn new(pool: DbPool, config: Arc<Config>) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("http client");
        Self { pool, config, http }
    }
}
