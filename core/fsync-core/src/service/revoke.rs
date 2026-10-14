//! Server-side revocation of a refresh token on logout, retried while the backend is unreachable.
//!
//! Tokens live in the app's keystore and are never written by core, so a revocation that cannot
//! be delivered is kept in memory only: it is retried with backoff for as long as the service
//! lives. If the app quits first the refresh token stays valid until its own 30 day expiry
//! (access tokens last 15 minutes).

use super::inner::{AbortOnDrop, Inner, lock};
use crate::error::CoreError;
use std::{sync::Arc, time::Duration};

/// Inline attempts before logout returns (waits 0.5 s and 1.5 s between them).
const INLINE_BACKOFF: [Duration; 2] = [Duration::from_millis(500), Duration::from_millis(1500)];
/// Background retries: delay doubles from the base up to `MAX_DELAY`, then stops after this many.
const BACKGROUND_TRIES: u32 = 24;
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
    /// Revoke `refresh`; if the backend cannot be reached, keep trying in the background.
    pub(super) async fn revoke_or_queue(self: &Arc<Self>, refresh: String) {
        let mut result = self.api.revoke(&refresh).await;
        for pause in INLINE_BACKOFF {
            match &result {
                Err(e) if transient(e) => {
                    tokio::time::sleep(pause).await;
                    result = self.api.revoke(&refresh).await;
                }
                _ => break,
            }
        }
        let Err(e) = result else { return };
        if !transient(&e) {
            tracing::warn!("logout: server refused to revoke the session: {e}");
            return;
        }
        tracing::warn!("logout: backend unreachable, revocation queued: {e}");
        let (api, base) = (self.api.clone(), self.revoke_base());
        let task = tokio::spawn(async move {
            let mut delay = base;
            for _ in 0..BACKGROUND_TRIES {
                tokio::time::sleep(delay).await;
                match api.revoke(&refresh).await {
                    Err(e) if transient(&e) => delay = (delay * 2).min(MAX_DELAY),
                    _ => return,
                }
            }
            tracing::warn!("logout: gave up revoking the session; it expires on its own");
        });
        if let Ok(mut q) = lock(&self.revokes) {
            q.retain(|t| !t.0.is_finished());
            q.push(AbortOnDrop(task));
        }
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
