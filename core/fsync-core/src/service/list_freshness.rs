//! When the pinned device list was last confirmed against the backend, and the offline policy
//! built on it: a device may start from its cached list while the backend is unreachable, but
//! only for a bounded time (a blocked or intercepted backend must not freeze revocations forever)
//! and only for a plain "cannot reach it" error.
//!
//! Kept in its own small file (`list-synced-at`, unix seconds) so the trust store format is
//! untouched. `TrustManager` calls `stamp` after every successful backend sync (one choke point).

use crate::{error::CoreError, fs_util::write_private_atomic, history::DB_FILE};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) const FILE: &str = "list-synced-at";
/// Longest an unconfirmed list may be used offline.
pub(super) const MAX_AGE_SECS: u64 = 7 * 24 * 3600;
/// A stamp further ahead of the clock than this is not believable (clock jump, tampering).
pub(super) const MAX_SKEW_SECS: u64 = 5 * 60;

#[derive(Clone)]
pub(super) struct ListFreshness {
    stamp_file: PathBuf,
    /// The history database, which holds the pinned list: its mtime starts the age clock of an
    /// install that predates the stamp file.
    db_file: PathBuf,
}

pub(super) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl ListFreshness {
    pub(super) fn new(data_dir: &Path) -> Self {
        Self {
            stamp_file: data_dir.join(FILE),
            db_file: data_dir.join(DB_FILE),
        }
    }

    /// Record that the list was just confirmed. A failed write only shortens the offline window,
    /// so it is logged, not fatal.
    pub(super) fn stamp(&self) {
        if let Err(e) = write_private_atomic(&self.stamp_file, unix_now().to_string().as_bytes()) {
            tracing::warn!("could not record the device list sync time: {e}");
        }
    }

    /// The recorded confirmation time. `None`: never recorded, unreadable, or dated more than
    /// `MAX_SKEW_SECS` into the future (not believable, so it must not read as "just now").
    pub(super) fn last(&self, now: u64) -> Option<u64> {
        let t: u64 = std::fs::read_to_string(&self.stamp_file)
            .ok()?
            .trim()
            .parse()
            .ok()?;
        (t <= now.saturating_add(MAX_SKEW_SECS)).then_some(t)
    }

    /// Start of the age clock: the stamp, else (install older than the stamp file) the mtime of
    /// the history database, else nothing, which fails closed. The fallback never grants a fresh
    /// window: a legacy install is as old as its database file, and a future-dated mtime is
    /// refused like a future stamp.
    fn clock_start(&self, now: u64) -> Option<u64> {
        self.last(now).or_else(|| {
            let t = std::fs::metadata(&self.db_file)
                .ok()?
                .modified()
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_secs();
            (t <= now.saturating_add(MAX_SKEW_SECS)).then_some(t)
        })
    }

    /// Whether the cached list may be used now after `err`. Returns the age in seconds of the
    /// last confirmation when it may.
    pub(super) fn offline_ok(&self, err: &CoreError, now: u64) -> Option<u64> {
        age_if_allowed(err, self.clock_start(now)?, now)
    }
}

/// The policy: only a plain unreachable-backend error, and only within `MAX_AGE_SECS`.
/// TLS or certificate failures, bad or oversized bodies and any HTTP status never qualify. That
/// includes 502/503/504 from a proxy in front of the backend: they stay `CoreError::Api` and fail
/// closed, because an intercepting proxy can answer exactly those.
pub(super) fn age_if_allowed(err: &CoreError, last: u64, now: u64) -> Option<u64> {
    if !matches!(err, CoreError::Unreachable(_)) {
        return None;
    }
    let age = now.saturating_sub(last);
    (age <= MAX_AGE_SECS).then_some(age)
}

#[cfg(test)]
#[path = "list_freshness_tests.rs"]
mod tests;
