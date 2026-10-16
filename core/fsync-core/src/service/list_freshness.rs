//! When the pinned device list was last confirmed against the backend, and the offline policy
//! built on it: a device may start from its cached list while the backend is unreachable, but
//! only for a bounded time (a blocked or intercepted backend must not freeze revocations forever)
//! and only for a plain "cannot reach it" error.
//!
//! Kept in its own small file (`list-synced-at`, unix seconds) so the trust store format is
//! untouched.

use crate::{error::CoreError, fs_util::write_private_atomic};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const FILE: &str = "list-synced-at";
/// Longest an unconfirmed list may be used offline.
pub(super) const MAX_AGE_SECS: u64 = 7 * 24 * 3600;

pub(super) struct ListFreshness(PathBuf);

pub(super) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl ListFreshness {
    pub(super) fn new(data_dir: &Path) -> Self {
        Self(data_dir.join(FILE))
    }

    /// Record that the list was just confirmed. A failed write only shortens the offline window,
    /// so it is logged, not fatal.
    pub(super) fn stamp(&self) {
        if let Err(e) = write_private_atomic(&self.0, unix_now().to_string().as_bytes()) {
            tracing::warn!("could not record the device list sync time: {e}");
        }
    }

    /// `None`: never recorded (older install) or unreadable.
    pub(super) fn last(&self) -> Option<u64> {
        std::fs::read_to_string(&self.0).ok()?.trim().parse().ok()
    }

    /// Whether the cached list may be used now after `err`. Returns the age in seconds of the
    /// last confirmation when it may. An install with no record starts its clock here.
    pub(super) fn offline_ok(&self, err: &CoreError, now: u64) -> Option<u64> {
        let last = match self.last() {
            Some(l) => l,
            None => {
                self.stamp();
                now
            }
        };
        age_if_allowed(err, last, now)
    }
}

/// The policy: only a plain unreachable-backend error, and only within `MAX_AGE_SECS`.
/// TLS or certificate failures, bad or oversized bodies and any HTTP status never qualify.
fn age_if_allowed(err: &CoreError, last: u64, now: u64) -> Option<u64> {
    if !matches!(err, CoreError::Unreachable(_)) {
        return None;
    }
    let age = now.saturating_sub(last);
    (age <= MAX_AGE_SECS).then_some(age)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ApiError;

    const DAY: u64 = 24 * 3600;

    fn unreachable() -> CoreError {
        CoreError::Unreachable("connection refused".into())
    }

    #[test]
    fn unreachable_within_the_bound_is_allowed() {
        assert_eq!(age_if_allowed(&unreachable(), 100, 100 + DAY), Some(DAY));
        assert!(age_if_allowed(&unreachable(), 0, MAX_AGE_SECS).is_some());
    }

    #[test]
    fn past_the_bound_is_refused() {
        assert_eq!(age_if_allowed(&unreachable(), 0, MAX_AGE_SECS + 1), None);
    }

    #[test]
    fn suspicious_errors_are_never_allowed() {
        let api = CoreError::Api(ApiError {
            status: 503,
            code: "x".into(),
            message: String::new(),
        });
        for e in [
            CoreError::Http("error sending request".into()),
            CoreError::Http("bad response body: eof".into()),
            CoreError::Http("response too large".into()),
            CoreError::Net("x".into()),
            api,
            CoreError::AuthExpired,
        ] {
            assert_eq!(age_if_allowed(&e, 10, 11), None, "{e:?}");
        }
    }

    #[test]
    fn stamp_round_trips_and_a_missing_record_starts_the_clock() {
        let dir = tempfile::tempdir().unwrap();
        let f = ListFreshness::new(dir.path());
        assert_eq!(f.last(), None);
        assert_eq!(f.offline_ok(&unreachable(), unix_now()), Some(0));
        let first = f.last().expect("clock started");
        std::fs::write(dir.path().join(FILE), "garbage").unwrap();
        assert_eq!(f.last(), None);
        f.stamp();
        assert!(f.last().unwrap() >= first);
    }
}
