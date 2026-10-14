//! Waiting for every handle to the engine to drop at stop, with bounded waits.

use std::{sync::Arc, time::Duration};

/// Upper bound on `Inner::stop` as enforced by service teardown (`FsyncService::drop`).
pub(super) const TEARDOWN_STOP_TIMEOUT: Duration = Duration::from_secs(3);

/// How long `stop` waits inline for aborted tasks to release their engine handles. Kept below
/// `TEARDOWN_STOP_TIMEOUT` (1 s margin): when teardown cancels `stop` at its timeout, the engine
/// and data-dir lock held by that future are dropped, so the wait must end, and hand a lingering
/// engine to the background holder, before that cancellation.
/// The 1 s margin is the budget for the graceful `shutdown` (router and endpoint close) that
/// follows a successful wait; when it does not fit, teardown cancels it and the engine is
/// dropped, which is the accepted worst case at process exit.
pub(super) const INLINE_RELEASE_WAIT: Duration =
    TEARDOWN_STOP_TIMEOUT.saturating_sub(Duration::from_secs(1));
const _: () = assert!(INLINE_RELEASE_WAIT.as_millis() < TEARDOWN_STOP_TIMEOUT.as_millis());

/// How long the background holder keeps `<data_dir>/engine.lock` for a leaked engine handle
/// before releasing the lock anyway, so a leak cannot block restart (login) forever.
pub(super) const LINGERING_LOCK_MAX_HOLD: Duration = Duration::from_secs(60);

pub(super) const POLL_INLINE: Duration = Duration::from_millis(100);
pub(super) const POLL_LINGERING: Duration = Duration::from_millis(250);

/// Polls until `shared` is the only handle, or `max_wait` passes (then returns it back).
pub(super) async fn wait_unique<T>(
    mut shared: Arc<T>,
    poll: Duration,
    max_wait: Duration,
) -> Result<T, Arc<T>> {
    let deadline = tokio::time::Instant::now() + max_wait;
    loop {
        match Arc::try_unwrap(shared) {
            Ok(v) => return Ok(v),
            Err(still_shared) => shared = still_shared,
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(shared);
        }
        tokio::time::sleep(poll).await;
    }
}

/// Holds `data_lock` until `shared` is unshared (then `shutdown`) or `max_hold` passes. On
/// timeout the leaked value is closed through the shared handle (`close`: its node stops
/// listening and its tasks end, so nothing live is left behind the lock) and only then is the
/// lock released, with a warning.
pub(super) async fn hold_lock_until_released<T, F1, F2>(
    shared: Arc<T>,
    data_lock: Option<std::fs::File>,
    poll: Duration,
    max_hold: Duration,
    shutdown: impl FnOnce(T) -> F1,
    close: impl FnOnce(Arc<T>) -> F2,
) where
    F1: std::future::Future<Output = ()>,
    F2: std::future::Future<Output = ()>,
{
    match wait_unique(shared, poll, max_hold).await {
        Ok(v) => {
            shutdown(v).await;
            drop(data_lock);
        }
        Err(leaked) => {
            close(leaked).await;
            drop(data_lock);
            tracing::warn!(
                "engine handle leaked: closed the engine and released engine.lock after {max_hold:?}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn locked_file(dir: &std::path::Path) -> std::fs::File {
        let f = std::fs::File::create(dir.join("engine.lock")).unwrap();
        f.try_lock().unwrap();
        f
    }

    fn lock_is_free(dir: &std::path::Path) -> bool {
        let f = std::fs::File::create(dir.join("engine.lock")).unwrap();
        f.try_lock().is_ok()
    }

    #[tokio::test]
    async fn leaked_handle_releases_lock_after_bounded_hold() {
        let dir = tempfile::tempdir().unwrap();
        let held = Arc::new(1u8);
        let _leak = held.clone();
        let lock = locked_file(dir.path());
        let closed = Arc::new(AtomicBool::new(false));
        let closed2 = closed.clone();
        assert!(!lock_is_free(dir.path()));
        hold_lock_until_released(
            held,
            Some(lock),
            Duration::from_millis(5),
            Duration::from_millis(50),
            |_| async { panic!("must not shut down a still-shared value") },
            |_| async move { closed2.store(true, Ordering::SeqCst) },
        )
        .await;
        assert!(closed.load(Ordering::SeqCst), "leaked value is closed");
        assert!(lock_is_free(dir.path()));
    }

    #[tokio::test]
    async fn released_handle_shuts_down_then_frees_lock() {
        let dir = tempfile::tempdir().unwrap();
        let lock = locked_file(dir.path());
        let shut = Arc::new(AtomicBool::new(false));
        let flag = shut.clone();
        hold_lock_until_released(
            Arc::new(1u8),
            Some(lock),
            Duration::from_millis(5),
            Duration::from_secs(5),
            |_| async move { flag.store(true, Ordering::SeqCst) },
            |_| async { panic!("an unshared value is shut down, not closed") },
        )
        .await;
        assert!(shut.load(Ordering::SeqCst));
        assert!(lock_is_free(dir.path()));
    }

    #[tokio::test]
    async fn returns_value_once_other_handles_drop() {
        let a = Arc::new(7u8);
        let b = a.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            drop(b);
        });
        let got = wait_unique(a, Duration::from_millis(5), Duration::from_secs(5)).await;
        assert_eq!(got.ok(), Some(7));
    }

    #[tokio::test]
    async fn gives_up_on_leaked_handle() {
        let a = Arc::new(7u8);
        let _leak = a.clone();
        let started = tokio::time::Instant::now();
        let got = wait_unique(a, Duration::from_millis(5), Duration::from_millis(60)).await;
        assert!(got.is_err());
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
