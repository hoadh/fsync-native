//! Server-side revocation of a refresh token on logout.
//!
//! `logout()` is a blocking call that apps may make from a UI thread, so the inline phase is a
//! single attempt bounded by `INLINE_BUDGET` (it covers refused, black-holed and slow
//! backends alike). Anything not answered in time is retried in the background, with the delay
//! doubling up to `MAX_DELAY`, until the backend answers or the service is dropped.
//!
//! Tokens live in the app's keystore and are never written by core, so a pending revocation
//! exists in memory only: if the app quits first, the refresh token stays valid until its own
//! 30 day expiry (access tokens last 15 minutes). Hence `logout_confirmed` (tells the app
//! whether the revocation settled) and `revoke_session` (engine-free retry at next launch).

use super::inner::{AbortOnDrop, Inner, lock};
use crate::error::CoreError;
use std::{sync::Arc, time::Duration};

/// How long `logout()` waits for the backend before handing the revocation to the background.
const INLINE_BUDGET: Duration = Duration::from_secs(3);
const MAX_DELAY: Duration = Duration::from_secs(300);
pub(super) const DEFAULT_BASE: Duration = Duration::from_secs(5);

/// Worth retrying: no answer, server trouble or throttling. Any other answer is final.
fn transient(e: &CoreError) -> bool {
    match e {
        CoreError::Api(a) => a.status >= 500 || a.status == 408 || a.status == 429,
        CoreError::Http(_) => true,
        _ => false,
    }
}

impl Inner {
    /// Revoke `refresh`; if the backend does not answer in time, keep trying in the background.
    /// `true`: settled (revoked, already dead, or refused for good: a retry cannot help).
    /// `false`: queued in memory only; the app must keep the token to retry (`revoke_session`).
    pub(super) async fn revoke_or_queue(self: &Arc<Self>, refresh: String) -> bool {
        let first = tokio::time::timeout(INLINE_BUDGET, self.api.revoke(&refresh)).await;
        match first {
            Ok(Ok(())) => return true,
            Ok(Err(e)) if !transient(&e) => {
                tracing::warn!("logout: server refused to revoke the session: {e}");
                return true;
            }
            Ok(Err(e)) => tracing::warn!("logout: revocation queued: {e}"),
            Err(_) => tracing::warn!("logout: backend did not answer, revocation queued"),
        }
        let (api, base) = (self.api.clone(), self.revoke_base());
        let task = tokio::spawn(async move {
            let mut delay = base;
            loop {
                tokio::time::sleep(delay).await;
                match api.revoke(&refresh).await {
                    Err(e) if transient(&e) => delay = (delay * 2).min(MAX_DELAY),
                    _ => return,
                }
            }
        });
        if let Ok(mut q) = lock(&self.revokes) {
            q.retain(|t| !t.0.is_finished());
            q.push(AbortOnDrop(task));
        }
        false
    }

    fn revoke_base(&self) -> Duration {
        lock(&self.revoke_base).map_or(DEFAULT_BASE, |b| *b)
    }
}

#[cfg(any(test, feature = "test-hooks"))]
impl super::FsyncService {
    /// Test hook: first retry delay of a queued logout revocation.
    #[doc(hidden)]
    pub fn set_revoke_retry_base(&self, d: Duration) {
        if let Ok(mut b) = self.inner.revoke_base.lock() {
            *b = d;
        }
    }
}
