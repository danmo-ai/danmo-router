//! Per-process admission. One account, one counter. No hash buckets and no shared store.
//!
//! Verified `sk-v1` keys spend that account's concurrency and QPS plus the process totals.
//! Legacy `sk-` hex keys spend only the small unverified budget until the handler confirms
//! the account and moves the permit. Forged keys never reach this type.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use danmo_core::config::Config;
use danmo_core::protocol::Protocol;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const IDLE_AFTER: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
pub struct Limits {
    pub user_concurrency: u32,
    pub instance_concurrency: u32,
    pub user_qps: u32,
    pub instance_qps: u32,
    pub legacy_concurrency: u32,
    pub legacy_qps: u32,
}

impl Limits {
    pub fn from_config(config: &Config) -> Self {
        Self {
            user_concurrency: config.relay_user_concurrency,
            instance_concurrency: config.relay_instance_concurrency,
            user_qps: config.relay_user_qps,
            instance_qps: config.relay_instance_qps,
            legacy_concurrency: config.relay_legacy_concurrency,
            legacy_qps: config.relay_legacy_qps,
        }
    }
}

#[derive(Clone)]
pub struct Admitter {
    inner: Arc<Mutex<Inner>>,
    limits: Limits,
}

struct Inner {
    instance_in_flight: u32,
    legacy_in_flight: u32,
    instance_qps: Bucket,
    legacy_qps: Bucket,
    users: HashMap<i64, UserSlot>,
}

struct UserSlot {
    in_flight: u32,
    qps: Bucket,
    last: Instant,
}

struct Bucket {
    tokens: f64,
    updated: Instant,
}

impl Bucket {
    fn full(rate: u32, now: Instant) -> Self {
        Self {
            tokens: rate as f64,
            updated: now,
        }
    }

    fn refill(&mut self, rate: f64, now: Instant) {
        if rate <= 0.0 {
            self.tokens = 0.0;
            self.updated = now;
            return;
        }
        let elapsed = now.saturating_duration_since(self.updated).as_secs_f64();
        self.tokens = (self.tokens + elapsed * rate).min(rate);
        self.updated = now;
    }

    fn available(&mut self, rate: f64, now: Instant) -> bool {
        self.refill(rate, now);
        self.tokens >= 1.0
    }

    fn take(&mut self) {
        self.tokens -= 1.0;
    }
}

enum Slot {
    Verified(i64),
    Legacy,
    Released,
}

/// Held from admission until the response finishes. Drop of the last clone releases the count.
///
/// `http::Extensions` requires `Clone`. Clones share one slot, so inserting the permit and
/// taking it back out does not admit the request twice.
#[derive(Clone)]
pub struct AdmitPermit {
    admit: Admitter,
    slot: Arc<Mutex<Slot>>,
}

#[derive(Debug)]
pub enum IntoAccountError {
    /// The account or the process is already at its cap.
    Limited,
    /// The signed account does not match the row the key hashes to.
    Mismatch,
}

impl Admitter {
    pub fn new(limits: Limits) -> Self {
        let now = Instant::now();
        Self {
            limits,
            inner: Arc::new(Mutex::new(Inner {
                instance_in_flight: 0,
                legacy_in_flight: 0,
                instance_qps: Bucket::full(limits.instance_qps, now),
                legacy_qps: Bucket::full(limits.legacy_qps, now),
                users: HashMap::new(),
            })),
        }
    }

    pub fn acquire_verified(&self, user_id: i64) -> Result<AdmitPermit, ()> {
        self.acquire(true, Some(user_id))
    }

    pub fn acquire_legacy(&self) -> Result<AdmitPermit, ()> {
        self.acquire(false, None)
    }

    pub fn user_in_flight(&self, user_id: i64) -> u32 {
        self.lock()
            .users
            .get(&user_id)
            .map(|slot| slot.in_flight)
            .unwrap_or(0)
    }

    pub fn instance_in_flight(&self) -> u32 {
        self.lock().instance_in_flight
    }

    pub fn legacy_in_flight(&self) -> u32 {
        self.lock().legacy_in_flight
    }

    fn acquire(&self, verified: bool, user_id: Option<i64>) -> Result<AdmitPermit, ()> {
        let mut inner = self.lock();
        let now = Instant::now();
        inner.evict(now);
        if verified {
            let user_id = user_id.expect("verified admission names an account");
            if inner.instance_in_flight >= self.limits.instance_concurrency {
                return Err(());
            }
            let in_flight = inner
                .users
                .get(&user_id)
                .map(|slot| slot.in_flight)
                .unwrap_or(0);
            if in_flight >= self.limits.user_concurrency {
                return Err(());
            }
            let user_rate = self.limits.user_qps as f64;
            let instance_rate = self.limits.instance_qps as f64;
            let user_ok = {
                let slot = inner.users.entry(user_id).or_insert_with(|| UserSlot {
                    in_flight: 0,
                    qps: Bucket::full(self.limits.user_qps, now),
                    last: now,
                });
                slot.qps.available(user_rate, now)
            };
            if !user_ok || !inner.instance_qps.available(instance_rate, now) {
                return Err(());
            }
            inner
                .users
                .get_mut(&user_id)
                .expect("just inserted")
                .qps
                .take();
            inner.instance_qps.take();
            let slot = inner.users.get_mut(&user_id).expect("just inserted");
            slot.in_flight += 1;
            slot.last = now;
            inner.instance_in_flight += 1;
            drop(inner);
            Ok(self.permit(Slot::Verified(user_id)))
        } else {
            if inner.legacy_in_flight >= self.limits.legacy_concurrency {
                return Err(());
            }
            let rate = self.limits.legacy_qps as f64;
            if !inner.legacy_qps.available(rate, now) {
                return Err(());
            }
            inner.legacy_qps.take();
            inner.legacy_in_flight += 1;
            drop(inner);
            Ok(self.permit(Slot::Legacy))
        }
    }

    fn permit(&self, slot: Slot) -> AdmitPermit {
        AdmitPermit {
            admit: self.clone(),
            slot: Arc::new(Mutex::new(slot)),
        }
    }

    fn release(&self, slot: &Slot) {
        let mut inner = self.lock();
        let now = Instant::now();
        match slot {
            Slot::Verified(user_id) => {
                inner.instance_in_flight = inner.instance_in_flight.saturating_sub(1);
                if let Some(user) = inner.users.get_mut(user_id) {
                    user.in_flight = user.in_flight.saturating_sub(1);
                    user.last = now;
                }
            }
            Slot::Legacy => {
                inner.legacy_in_flight = inner.legacy_in_flight.saturating_sub(1);
            }
            Slot::Released => {}
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|err| err.into_inner())
    }
}

impl Inner {
    fn evict(&mut self, now: Instant) {
        self.users.retain(|_, slot| {
            slot.in_flight > 0 || now.saturating_duration_since(slot.last) < IDLE_AFTER
        });
    }
}

impl AdmitPermit {
    /// Legacy permits become the account permit after the key is found.
    /// Verified permits must already name that same account.
    pub fn confirm_account(&self, user_id: i64) -> Result<(), IntoAccountError> {
        let mut slot = self.lock_slot();
        match *slot {
            Slot::Verified(id) if id == user_id => Ok(()),
            Slot::Verified(_) => {
                let taken = std::mem::replace(&mut *slot, Slot::Released);
                drop(slot);
                self.admit.release(&taken);
                Err(IntoAccountError::Mismatch)
            }
            Slot::Legacy => {
                let taken = std::mem::replace(&mut *slot, Slot::Released);
                drop(slot);
                self.admit.release(&taken);
                let acquired = self
                    .admit
                    .acquire_verified(user_id)
                    .map_err(|_| IntoAccountError::Limited)?;
                let moved = {
                    let mut stolen = acquired.lock_slot();
                    std::mem::replace(&mut *stolen, Slot::Released)
                };
                drop(acquired);
                *self.lock_slot() = moved;
                Ok(())
            }
            Slot::Released => Err(IntoAccountError::Mismatch),
        }
    }

    fn lock_slot(&self) -> std::sync::MutexGuard<'_, Slot> {
        self.slot.lock().unwrap_or_else(|err| err.into_inner())
    }
}

impl Drop for AdmitPermit {
    fn drop(&mut self) {
        if Arc::strong_count(&self.slot) != 1 {
            return;
        }
        let mut slot = self.lock_slot();
        let taken = std::mem::replace(&mut *slot, Slot::Released);
        drop(slot);
        self.admit.release(&taken);
    }
}

/// Removes the permit middleware stored on the request. `Extension` would clone it and release twice.
pub struct TakePermit(pub AdmitPermit);

impl<S> FromRequestParts<S> for TakePermit
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .remove::<AdmitPermit>()
            .map(TakePermit)
            .ok_or_else(|| {
                crate::error::protocol_error(
                    Protocol::OpenaiChat,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "missing admission permit",
                    "internal_error",
                )
                .into_response()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits {
            user_concurrency: 2,
            instance_concurrency: 3,
            user_qps: 10,
            instance_qps: 10,
            legacy_concurrency: 1,
            legacy_qps: 10,
        }
    }

    #[test]
    fn same_account_shares_one_concurrency_counter() {
        let admit = Admitter::new(limits());
        let a = admit.acquire_verified(7).unwrap();
        let b = admit.acquire_verified(7).unwrap();
        assert!(admit.acquire_verified(7).is_err());
        assert_eq!(admit.user_in_flight(7), 2);
        assert_eq!(admit.instance_in_flight(), 2);
        drop(a);
        assert_eq!(admit.user_in_flight(7), 1);
        assert!(admit.acquire_verified(7).is_ok());
        drop(b);
    }

    #[test]
    fn instance_cap_counts_every_account() {
        let admit = Admitter::new(Limits {
            instance_concurrency: 2,
            user_concurrency: 10,
            ..limits()
        });
        let _a = admit.acquire_verified(1).unwrap();
        let _b = admit.acquire_verified(2).unwrap();
        assert!(admit.acquire_verified(3).is_err());
        assert_eq!(admit.instance_in_flight(), 2);
    }

    #[test]
    fn qps_is_not_spent_when_concurrency_is_full() {
        let admit = Admitter::new(Limits {
            user_concurrency: 1,
            user_qps: 2,
            instance_qps: 10,
            instance_concurrency: 10,
            ..limits()
        });
        let held = admit.acquire_verified(4).unwrap();
        assert!(admit.acquire_verified(4).is_err());
        drop(held);
        let again = admit.acquire_verified(4).unwrap();
        drop(again);
        assert!(admit.acquire_verified(4).is_err());
    }

    #[test]
    fn qps_burst_is_the_per_second_limit() {
        let admit = Admitter::new(Limits {
            user_qps: 1,
            user_concurrency: 10,
            instance_qps: 10,
            instance_concurrency: 10,
            ..limits()
        });
        let first = admit.acquire_verified(4).unwrap();
        drop(first);
        assert!(admit.acquire_verified(4).is_err());
        assert_eq!(admit.user_in_flight(4), 0);
    }

    #[test]
    fn legacy_budget_does_not_touch_verified_counters() {
        let admit = Admitter::new(limits());
        let held = admit.acquire_legacy().unwrap();
        assert!(admit.acquire_legacy().is_err());
        assert_eq!(admit.legacy_in_flight(), 1);
        assert_eq!(admit.instance_in_flight(), 0);
        assert_eq!(admit.user_in_flight(1), 0);
        drop(held);
        assert_eq!(admit.legacy_in_flight(), 0);
        assert!(admit.acquire_verified(1).is_ok());
    }

    #[test]
    fn legacy_permit_moves_onto_the_account() {
        let admit = Admitter::new(Limits {
            user_concurrency: 1,
            ..limits()
        });
        let legacy = admit.acquire_legacy().unwrap();
        legacy.confirm_account(9).unwrap();
        assert_eq!(admit.legacy_in_flight(), 0);
        assert_eq!(admit.user_in_flight(9), 1);
        assert!(admit.acquire_verified(9).is_err());
        drop(legacy);
        assert_eq!(admit.user_in_flight(9), 0);
    }
}
