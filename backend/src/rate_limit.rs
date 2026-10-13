//! In-memory sliding-window rate limiter (single instance MVP).
//!
//! Each key carries its own window, so expiry is per key. Memory is bounded: expired keys are
//! pruned at most once per `PRUNE_EVERY` (amortized O(1) per call) and the map never exceeds
//! `MAX_KEYS` (when full an arbitrary key is evicted, its owner's counter resetting; the
//! expired-key scan at capacity runs at most once per second).

use crate::error::{ApiError, ApiResult};
use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::{Duration, Instant},
};

const MAX_KEYS: usize = 100_000;
const PRUNE_EVERY: Duration = Duration::from_secs(30);
const SCAN_MIN_GAP: Duration = Duration::from_secs(1);

struct Bucket {
    hits: VecDeque<Instant>,
    window: Duration,
}

impl Bucket {
    fn expired(&self, now: Instant) -> bool {
        self.hits
            .back()
            .is_none_or(|t| now.duration_since(*t) >= self.window)
    }
}

struct Inner {
    map: HashMap<String, Bucket>,
    last_prune: Instant,
    /// Last time the map was scanned for expired keys (periodic prune or at-capacity scan).
    last_scan: Option<Instant>,
    max_keys: usize,
}

pub struct RateLimiter(Mutex<Inner>);

impl Default for RateLimiter {
    fn default() -> Self {
        Self::with_max_keys(MAX_KEYS)
    }
}

impl Inner {
    /// Drop expired keys; at capacity, scan at most once per `SCAN_MIN_GAP` (a full scan is O(n),
    /// so an attacker filling the map must not trigger one per request).
    fn make_room(&mut self, now: Instant) {
        if now.duration_since(self.last_prune) >= PRUNE_EVERY {
            self.map.retain(|_, b| !b.expired(now));
            self.last_prune = now;
            self.last_scan = Some(now);
        }
        if self.map.len() < self.max_keys {
            return;
        }
        if self
            .last_scan
            .is_none_or(|t| now.duration_since(t) >= SCAN_MIN_GAP)
        {
            self.map.retain(|_, b| !b.expired(now));
            self.last_scan = Some(now);
        }
        if self.map.len() >= self.max_keys
            && let Some(victim) = self.map.keys().next().cloned()
        {
            self.map.remove(&victim);
        }
    }

    /// Whether `key` is under `limit` right now; `Err` carries the wait.
    fn peek(&mut self, key: &str, limit: usize, window: Duration, now: Instant) -> ApiResult<()> {
        let Some(b) = self.map.get_mut(key) else {
            return Ok(());
        };
        b.window = window;
        while b
            .hits
            .front()
            .is_some_and(|t| now.duration_since(*t) >= window)
        {
            b.hits.pop_front();
        }
        if b.hits.len() >= limit {
            let oldest = b.hits.front().copied().unwrap_or(now);
            let wait = window.saturating_sub(now.duration_since(oldest));
            return Err(ApiError::RateLimited {
                retry_after_s: wait.as_secs().max(1),
            });
        }
        Ok(())
    }

    fn record(&mut self, key: &str, window: Duration, now: Instant) {
        if !self.map.contains_key(key) {
            self.make_room(now);
        }
        let b = self.map.entry(key.to_owned()).or_insert_with(|| Bucket {
            hits: VecDeque::new(),
            window,
        });
        b.window = window;
        b.hits.push_back(now);
    }
}

impl RateLimiter {
    pub fn with_max_keys(max_keys: usize) -> Self {
        Self(Mutex::new(Inner {
            map: HashMap::new(),
            last_prune: Instant::now(),
            last_scan: None,
            max_keys: max_keys.max(1),
        }))
    }

    pub fn len(&self) -> usize {
        self.lock().map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Record a hit for `key`; error if `limit` hits already happened within `window`.
    pub fn check(&self, key: &str, limit: usize, window: Duration) -> ApiResult<()> {
        let now = Instant::now();
        let mut g = self.lock();
        g.peek(key, limit, window, now)?;
        g.record(key, window, now);
        Ok(())
    }

    /// Like `check` but records nothing: pair with `record` once the guarded action succeeded.
    pub fn peek(&self, key: &str, limit: usize, window: Duration) -> ApiResult<()> {
        self.lock().peek(key, limit, window, Instant::now())
    }

    /// Count a hit without checking the limit.
    pub fn record(&self, key: &str, window: Duration) {
        self.lock().record(key, window, Instant::now());
    }
}

#[cfg(test)]
mod tests;
