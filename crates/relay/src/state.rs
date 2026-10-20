use crate::admit::{Admitter, Limits};
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
    pub admit: Admitter,
}

impl RelayState {
    pub fn new(pool: DbPool, config: Arc<Config>) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("http client");
        let health = Arc::new(ChannelHealth::new(config.health.clone()));
        let admit = Admitter::new(Limits::from_config(&config));
        Self {
            pool,
            config,
            http,
            health,
            admit,
        }
    }

    pub fn user_in_flight(&self, user_id: i64) -> u32 {
        self.admit.user_in_flight(user_id)
    }

    pub fn instance_in_flight(&self) -> u32 {
        self.admit.instance_in_flight()
    }

    pub fn legacy_in_flight(&self) -> u32 {
        self.admit.legacy_in_flight()
    }
}
