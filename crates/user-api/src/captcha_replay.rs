//! Process-local single-use tracking for captcha nonces.
//!
//! Avoids writing challenges to the shared database. Multi-replica `danmo-user`
//! may accept the same token once per process within the captcha TTL; keep a
//! single user-api instance (or put a shared edge rate limit in front) if that matters.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const MAX_ENTRIES: usize = 10_000;
const RETAIN: Duration = Duration::from_secs(600);

#[derive(Default)]
pub struct CaptchaReplayGuard {
    inner: Mutex<HashMap<String, Instant>>,
}

impl CaptchaReplayGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if this nonce was already consumed.
    pub fn consume(&self, nonce: &str) -> bool {
        let now = Instant::now();
        let mut map = self.inner.lock().expect("captcha replay lock");
        map.retain(|_, at| now.duration_since(*at) < RETAIN);
        if map.len() >= MAX_ENTRIES {
            // Under flood, drop oldest half rather than grow without bound.
            let mut pairs: Vec<_> = map.iter().map(|(k, v)| (k.clone(), *v)).collect();
            pairs.sort_by_key(|(_, at)| *at);
            for (k, _) in pairs.into_iter().take(MAX_ENTRIES / 2) {
                map.remove(&k);
            }
        }
        if map.contains_key(nonce) {
            return true;
        }
        map.insert(nonce.to_string(), now);
        false
    }
}
