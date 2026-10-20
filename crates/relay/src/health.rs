//! Per-process channel health, keyed by (channel, model).
//!
//! Two signals, modelled on upstream outlier detection in service meshes:
//! - Consecutive failures (connect error, 5xx, broken stream) eject the channel for a while.
//!   Once the ejection lapses the entry is half-open and exactly one request probes it; a failed
//!   probe ejects again for twice as long, capped. Any success closes the breaker.
//! - A 429 halves the channel's weight factor (down to a floor), which climbs back linearly
//!   with time. A `Retry-After` additionally ejects the channel until then.
//!
//! State lives in memory and is not shared across relay replicas: each one learns on its own.
//! When every candidate is ejected the breakers are ignored rather than failing the request.

use danmo_core::config::HealthConfig;
use danmo_core::routing::{weighted_shuffle_by_priority, ChannelCandidate};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// A claimed probe that never reports (e.g. the task died) frees up after this long; matches
/// the upstream request timeout.
const PROBE_TIMEOUT: Duration = Duration::from_secs(120);

type Key = (i64, String);

struct Entry {
    failures: u32,
    ejections: u32,
    /// Set while tripped. Once it lapses the entry is half-open until a result arrives.
    open_until: Option<Instant>,
    probe_until: Option<Instant>,
    factor: f64,
    factor_at: Instant,
}

impl Entry {
    fn new(now: Instant) -> Self {
        Self {
            failures: 0,
            ejections: 0,
            open_until: None,
            probe_until: None,
            factor: 1.0,
            factor_at: now,
        }
    }

    fn admits(&self, now: Instant) -> bool {
        !self.open_until.is_some_and(|t| t > now) && !self.probe_until.is_some_and(|t| t > now)
    }

    fn half_open(&self, now: Instant) -> bool {
        self.open_until.is_some_and(|t| t <= now)
    }

    fn factor(&self, cfg: &HealthConfig, now: Instant) -> f64 {
        if cfg.recover_secs == 0 {
            return 1.0;
        }
        let gained =
            now.saturating_duration_since(self.factor_at).as_secs_f64() / cfg.recover_secs as f64;
        (self.factor + gained).min(1.0)
    }
}

pub struct ChannelHealth {
    cfg: HealthConfig,
    entries: Mutex<HashMap<Key, Entry>>,
}

impl ChannelHealth {
    pub fn new(cfg: HealthConfig) -> Self {
        Self {
            cfg,
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<Key, Entry>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn eject_for(&self, ejections: u32) -> Duration {
        let doubled = self
            .cfg
            .base_eject_secs
            .saturating_mul(1u64 << ejections.saturating_sub(1).min(32));
        Duration::from_secs(doubled.min(self.cfg.max_eject_secs))
    }

    /// Pick the channel for the next attempt: drop ejected candidates, scale weights by the
    /// rate-limit factor, then shuffle by priority. A half-open pick claims the probe.
    /// Returns `None` only for an empty list.
    pub fn pick(
        &self,
        candidates: Vec<ChannelCandidate>,
        model: &str,
        now: Instant,
    ) -> Option<ChannelCandidate> {
        if !self.cfg.enabled {
            return weighted_shuffle_by_priority(candidates).into_iter().next();
        }
        let mut entries = self.lock();
        let key = |c: &ChannelCandidate| (c.channel.id, model.to_string());
        let (admitted, ejected): (Vec<_>, Vec<_>) = candidates
            .into_iter()
            .partition(|c| entries.get(&key(c)).is_none_or(|e| e.admits(now)));

        if admitted.is_empty() {
            if !ejected.is_empty() {
                warn!(model, "every channel is ejected, ignoring circuit breakers");
            }
            return weighted_shuffle_by_priority(ejected).into_iter().next();
        }

        let admitted = admitted
            .into_iter()
            .map(|mut c| {
                if let Some(e) = entries.get(&key(&c)) {
                    let scaled = c.effective_weight as f64 * e.factor(&self.cfg, now);
                    c.effective_weight = (scaled.round() as i64).max(1);
                }
                c
            })
            .collect();
        let picked = weighted_shuffle_by_priority(admitted).into_iter().next()?;
        if let Some(e) = entries.get_mut(&key(&picked)) {
            if e.half_open(now) {
                e.probe_until = Some(now + PROBE_TIMEOUT);
                info!(
                    channel_id = picked.channel.id,
                    model, "probing ejected channel"
                );
            }
        }
        Some(picked)
    }

    pub fn record_success(&self, channel_id: i64, model: &str, now: Instant) {
        if !self.cfg.enabled {
            return;
        }
        let mut entries = self.lock();
        let key = (channel_id, model.to_string());
        let Some(e) = entries.get_mut(&key) else {
            return;
        };
        if e.open_until.is_some() {
            info!(channel_id, model, "channel recovered");
        }
        e.failures = 0;
        e.ejections = 0;
        e.open_until = None;
        e.probe_until = None;
        if e.factor(&self.cfg, now) >= 1.0 {
            entries.remove(&key);
        }
    }

    pub fn record_failure(&self, channel_id: i64, model: &str, now: Instant) {
        if !self.cfg.enabled {
            return;
        }
        let mut entries = self.lock();
        let e = entries
            .entry((channel_id, model.to_string()))
            .or_insert_with(|| Entry::new(now));
        e.failures += 1;
        e.probe_until = None;
        let trip = match e.open_until {
            // Requests already in flight when the breaker opened.
            Some(t) if t > now => false,
            Some(_) => true,
            None => e.failures >= self.cfg.failure_threshold,
        };
        if trip {
            e.ejections += 1;
            let ttl = self.eject_for(e.ejections);
            e.open_until = Some(now + ttl);
            warn!(
                channel_id,
                model,
                failures = e.failures,
                eject_secs = ttl.as_secs(),
                "channel ejected"
            );
        }
    }

    pub fn record_rate_limited(
        &self,
        channel_id: i64,
        model: &str,
        retry_after: Option<Duration>,
        now: Instant,
    ) {
        if !self.cfg.enabled {
            return;
        }
        let mut entries = self.lock();
        let e = entries
            .entry((channel_id, model.to_string()))
            .or_insert_with(|| Entry::new(now));
        e.factor = (e.factor(&self.cfg, now) * 0.5).max(self.cfg.weight_floor);
        e.factor_at = now;
        e.probe_until = None;
        if let Some(d) = retry_after {
            let until = now + d.min(Duration::from_secs(self.cfg.max_eject_secs));
            if e.open_until.is_none_or(|t| t < until) {
                e.open_until = Some(until);
            }
        }
        warn!(
            channel_id,
            model,
            weight_factor = e.factor,
            retry_after_secs = retry_after.map(|d| d.as_secs()),
            "channel rate limited"
        );
    }

    #[cfg(test)]
    fn factor_of(&self, channel_id: i64, model: &str, now: Instant) -> f64 {
        self.lock()
            .get(&(channel_id, model.to_string()))
            .map_or(1.0, |e| e.factor(&self.cfg, now))
    }
}

/// `Retry-After` in delta-seconds form; HTTP dates are rare from LLM APIs and ignored.
pub fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use danmo_core::models::{Channel, ChannelModel};

    const M: &str = "m1";

    fn cand(id: i64, priority: i64) -> ChannelCandidate {
        let channel = Channel {
            id,
            name: format!("c{id}"),
            type_: "openai_compat".into(),
            key_enc: String::new(),
            base_url: "https://example.com".into(),
            group_name: "default".into(),
            priority,
            weight: 1,
            status: "enabled".into(),
            created_at: String::new(),
            updated_at: String::new(),
            relay_mode: "passthrough".into(),
            upstream_protocol: "openai_chat".into(),
            auth_style: "bearer".into(),
            inject_stream_usage: 1,
            default_dialect: "standard".into(),
            vendor: String::new(),
        };
        let row = ChannelModel {
            id,
            channel_id: id,
            model: M.into(),
            upstream_model: String::new(),
            upstream_dialect: "standard".into(),
        };
        ChannelCandidate::new(channel, row)
    }

    fn health() -> ChannelHealth {
        ChannelHealth::new(HealthConfig::default())
    }

    fn pick_id(h: &ChannelHealth, list: &[(i64, i64)], now: Instant) -> i64 {
        let candidates = list.iter().map(|&(id, p)| cand(id, p)).collect();
        h.pick(candidates, M, now).unwrap().channel.id
    }

    fn fail(h: &ChannelHealth, id: i64, times: u32, now: Instant) {
        for _ in 0..times {
            h.record_failure(id, M, now);
        }
    }

    const PAIR: &[(i64, i64)] = &[(1, 10), (2, 1)];

    #[test]
    fn trips_after_threshold() {
        let h = health();
        let t = Instant::now();
        fail(&h, 1, 4, t);
        assert_eq!(pick_id(&h, PAIR, t), 1);
        fail(&h, 1, 1, t);
        assert_eq!(pick_id(&h, PAIR, t), 2);
    }

    #[test]
    fn success_resets_failure_count() {
        let h = health();
        let t = Instant::now();
        fail(&h, 1, 4, t);
        h.record_success(1, M, t);
        fail(&h, 1, 4, t);
        assert_eq!(pick_id(&h, PAIR, t), 1);
    }

    #[test]
    fn half_open_admits_a_single_probe() {
        let h = health();
        let t = Instant::now();
        fail(&h, 1, 5, t);
        let later = t + Duration::from_secs(31);
        assert_eq!(pick_id(&h, PAIR, later), 1);
        assert_eq!(pick_id(&h, PAIR, later), 2);
        h.record_success(1, M, later);
        assert_eq!(pick_id(&h, PAIR, later), 1);
        assert_eq!(pick_id(&h, PAIR, later), 1);
    }

    #[test]
    fn failed_probe_doubles_ejection_up_to_cap() {
        let h = health();
        let mut t = Instant::now();
        fail(&h, 1, 5, t);
        let mut expect = 30;
        for _ in 0..6 {
            t += Duration::from_secs(expect);
            assert_eq!(pick_id(&h, PAIR, t), 1, "probe after {expect}s");
            h.record_failure(1, M, t);
            expect = (expect * 2).min(600);
            assert_eq!(pick_id(&h, PAIR, t + Duration::from_secs(expect - 1)), 2);
        }
        assert_eq!(expect, 600);
    }

    #[test]
    fn stragglers_do_not_extend_an_open_breaker() {
        let h = health();
        let t = Instant::now();
        fail(&h, 1, 5, t);
        fail(&h, 1, 3, t + Duration::from_secs(5));
        assert_eq!(pick_id(&h, PAIR, t + Duration::from_secs(30)), 1);
    }

    #[test]
    fn rate_limit_halves_weight_and_recovers_over_time() {
        let h = health();
        let t = Instant::now();
        h.record_rate_limited(1, M, None, t);
        assert_eq!(h.factor_of(1, M, t), 0.5);
        h.record_rate_limited(1, M, None, t);
        assert_eq!(h.factor_of(1, M, t), 0.25);
        for _ in 0..10 {
            h.record_rate_limited(1, M, None, t);
        }
        assert_eq!(h.factor_of(1, M, t), 0.05);
        let half = h.factor_of(1, M, t + Duration::from_secs(30));
        assert!((half - 0.55).abs() < 1e-9, "{half}");
        assert_eq!(h.factor_of(1, M, t + Duration::from_secs(60)), 1.0);
        // Rate limiting never ejects on its own.
        assert_eq!(pick_id(&h, PAIR, t), 1);
    }

    #[test]
    fn rate_limited_channel_gets_less_traffic() {
        let h = health();
        let t = Instant::now();
        for _ in 0..5 {
            h.record_rate_limited(1, M, None, t);
        }
        let same_tier = &[(1, 1), (2, 1)];
        let hits = (0..2000).filter(|_| pick_id(&h, same_tier, t) == 1).count();
        // Factor 1/32 vs 1: about 3% of picks.
        assert!(hits < 200, "{hits}");
    }

    #[test]
    fn retry_after_ejects_until_then() {
        let h = health();
        let t = Instant::now();
        h.record_rate_limited(1, M, Some(Duration::from_secs(10)), t);
        assert_eq!(pick_id(&h, PAIR, t + Duration::from_secs(9)), 2);
        assert_eq!(pick_id(&h, PAIR, t + Duration::from_secs(10)), 1);
    }

    #[test]
    fn lower_tier_is_used_before_falling_back() {
        let h = health();
        let t = Instant::now();
        fail(&h, 1, 5, t);
        fail(&h, 2, 5, t);
        assert_eq!(pick_id(&h, &[(1, 10), (2, 10), (3, 1)], t), 3);
    }

    #[test]
    fn all_ejected_falls_back_to_every_candidate() {
        let h = health();
        let t = Instant::now();
        fail(&h, 1, 5, t);
        fail(&h, 2, 5, t);
        assert_eq!(pick_id(&h, PAIR, t), 1);
    }

    #[test]
    fn breakers_are_per_model() {
        let h = health();
        let t = Instant::now();
        fail(&h, 1, 5, t);
        let picked = h.pick(vec![cand(1, 10), cand(2, 1)], "m2", t).unwrap();
        assert_eq!(picked.channel.id, 1);
    }

    #[test]
    fn disabled_is_a_no_op() {
        let h = ChannelHealth::new(HealthConfig {
            enabled: false,
            ..HealthConfig::default()
        });
        let t = Instant::now();
        fail(&h, 1, 50, t);
        h.record_rate_limited(1, M, Some(Duration::from_secs(60)), t);
        assert_eq!(pick_id(&h, PAIR, t), 1);
    }

    #[test]
    fn parses_retry_after_seconds() {
        let mut headers = reqwest::header::HeaderMap::new();
        assert_eq!(retry_after(&headers), None);
        headers.insert("retry-after", "7".parse().unwrap());
        assert_eq!(retry_after(&headers), Some(Duration::from_secs(7)));
        headers.insert(
            "retry-after",
            "Wed, 21 Oct 2015 07:28:00 GMT".parse().unwrap(),
        );
        assert_eq!(retry_after(&headers), None);
    }
}
