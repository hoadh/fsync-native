//! Hard logout: when the server-side revocation could not be settled before quit, the refresh
//! token is kept in the secrets store (`pending_revoke`, a small JSON document) and revoked at a
//! later launch with `revoke_session` (no engine needed).

use super::secrets_sync::{OP_SAVE_WAIT, persist, read};
use crate::core_state::Core;
use fsync_core::service::{FsyncError, REVOKE_SESSION_BUDGET, revoke_session};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Refresh tokens live 30 days; a revoke older than that has nothing left to revoke.
const MAX_AGE_SECS: u64 = 30 * 24 * 3600;
const MAX_FAILURES: u32 = 20;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Pending {
    pub api: String,
    pub refresh: String,
    pub since: u64,
    pub failures: u32,
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Pending {
    pub fn new(api: &str, refresh: &str, now: u64) -> Self {
        Self {
            api: api.into(),
            refresh: refresh.into(),
            since: now,
            failures: 0,
        }
    }

    pub fn is_expired(&self, now: u64) -> bool {
        now.saturating_sub(self.since) > MAX_AGE_SECS || self.failures >= MAX_FAILURES
    }

    /// What to keep after a revoke attempt: `None` = settled or hopeless (delete), `Some` = retry later.
    pub fn after_attempt(self, outcome: &Result<(), FsyncError>, now: u64) -> Option<Self> {
        match outcome {
            Ok(()) => None,
            Err(FsyncError::Network(_)) => {
                let next = Self {
                    failures: self.failures + 1,
                    ..self
                };
                (!next.is_expired(now)).then_some(next)
            }
            // A refusal a retry will not change (bad URL, malformed token).
            Err(_) => None,
        }
    }
}

/// The stored document: a JSON array of entries (one per sign-out not yet settled). A single
/// object from an earlier build still decodes.
pub fn decode_all(raw: &str) -> Vec<Pending> {
    if let Ok(list) = serde_json::from_str::<Vec<Pending>>(raw) {
        return list;
    }
    serde_json::from_str::<Pending>(raw).into_iter().collect()
}

pub fn encode_all(list: &[Pending]) -> Option<String> {
    // Plain strings and integers always serialize.
    (!list.is_empty()).then(|| serde_json::to_string(list).unwrap_or_default())
}

/// Stored value after adding `entry` (a new sign-out never replaces an older, unsettled one).
pub fn with_entry(stored: Option<&str>, entry: Pending) -> Option<String> {
    let mut list = stored.map(decode_all).unwrap_or_default();
    list.push(entry);
    encode_all(&list)
}

/// Stored value after an attempt on `attempted`: only that entry is replaced by `next` (or
/// removed); entries added meanwhile stay untouched.
pub fn with_attempt_applied(
    stored: Option<&str>,
    attempted: &Pending,
    next: Option<Pending>,
) -> Option<String> {
    let mut list = stored.map(decode_all).unwrap_or_default();
    let same = |p: &Pending| p.refresh == attempted.refresh && p.since == attempted.since;
    match (list.iter().position(same), next) {
        (Some(i), Some(n)) => list[i] = n,
        (Some(i), None) => {
            list.remove(i);
        }
        (None, _) => {}
    }
    encode_all(&list)
}

/// Revokes the tokens kept by earlier sign-outs. Blocking (up to ~10 s each): call off the UI
/// thread.
pub fn resume(core: &Core) {
    let Some(raw) = read(core, |s| s.pending_revoke.clone()).flatten() else {
        return;
    };
    let now = now_secs();
    for pending in decode_all(&raw) {
        let outcome = if pending.is_expired(now) {
            Ok(())
        } else {
            let insecure = fsync_linux_platform::config_file::is_loopback_http(&pending.api);
            revoke_session(
                &pending.api,
                &pending.refresh,
                insecure,
                REVOKE_SESSION_BUDGET,
            )
        };
        tracing::info!("pending revoke attempt finished: ok={}", outcome.is_ok());
        let next = pending.clone().after_attempt(&outcome, now);
        persist(core, OP_SAVE_WAIT, |s| {
            s.pending_revoke = with_attempt_applied(s.pending_revoke.as_deref(), &pending, next);
        });
    }
    // An unreadable document has nothing to retry. Decided on the value stored right now (a
    // sign-out may have appended an entry since `raw` was read).
    if decode_all(&raw).is_empty() {
        persist(core, OP_SAVE_WAIT, |s| {
            if s.pending_revoke
                .as_deref()
                .is_some_and(|v| decode_all(v).is_empty())
            {
                s.pending_revoke = None;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_failure_is_kept_and_counted_until_hopeless() {
        let p = Pending::new("https://api", "tok", 100);
        let kept = p
            .clone()
            .after_attempt(&Err(FsyncError::Network("x".into())), 200)
            .unwrap();
        assert_eq!(kept.failures, 1);
        let old = Pending::new("https://api", "tok", 0);
        assert!(
            old.after_attempt(&Err(FsyncError::Network("x".into())), MAX_AGE_SECS + 5)
                .is_none()
        );
    }

    #[test]
    fn success_and_final_refusal_delete_the_entry() {
        let p = Pending::new("https://api", "tok", 1);
        assert!(p.clone().after_attempt(&Ok(()), 2).is_none());
        assert!(
            p.after_attempt(&Err(FsyncError::Invalid("bad".into())), 2)
                .is_none()
        );
    }

    #[test]
    fn encode_decode_roundtrip_legacy_single_and_garbage() {
        let p = Pending::new("http://127.0.0.1:1", "r", 5);
        assert_eq!(
            decode_all(&encode_all(std::slice::from_ref(&p)).unwrap()),
            vec![p.clone()]
        );
        let legacy = serde_json::to_string(&p).unwrap();
        assert_eq!(decode_all(&legacy), vec![p]);
        assert!(decode_all("not json").is_empty());
        assert_eq!(encode_all(&[]), None);
    }

    #[test]
    fn new_logout_appends_and_an_attempt_touches_only_its_entry() {
        let a = Pending::new("https://api", "a", 1);
        let b = Pending::new("https://api", "b", 2);
        let stored = with_entry(None, a.clone());
        // Another sign-out lands while the attempt on `a` is in flight.
        let stored = with_entry(stored.as_deref(), b.clone());
        assert_eq!(
            decode_all(stored.as_deref().unwrap()),
            vec![a.clone(), b.clone()]
        );
        // `a` settled: only `a` goes away.
        let after = with_attempt_applied(stored.as_deref(), &a, None);
        assert_eq!(decode_all(after.as_deref().unwrap()), vec![b.clone()]);
        // `b` failed again: it is replaced in place, nothing else changes.
        let retry = Pending {
            failures: 1,
            ..b.clone()
        };
        let after = with_attempt_applied(after.as_deref(), &b, Some(retry.clone()));
        assert_eq!(decode_all(after.as_deref().unwrap()), vec![retry]);
        // Removing the last entry clears the field.
        assert_eq!(with_attempt_applied(after.as_deref(), &b, None), None);
    }
}
