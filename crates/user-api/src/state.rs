use danmo_core::{Config, DbPool};
use std::sync::Arc;

#[derive(Clone)]
pub struct UserState {
    pub pool: DbPool,
    pub config: Arc<Config>,
}

impl UserState {
    pub fn new(pool: DbPool, config: Arc<Config>) -> Self {
        Self { pool, config }
    }
}
