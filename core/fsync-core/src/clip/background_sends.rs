//! Clip sends that outlive the `publish` call that started them, owned by the engine so that
//! `Engine::close` cancels them instead of leaving them running past shutdown.

use std::{future::Future, sync::Mutex};
use tokio::task::{JoinError, JoinSet};

/// `None` once closed: nothing new is started after `close`.
pub(crate) struct BackgroundSends(Mutex<Option<JoinSet<()>>>);

impl Default for BackgroundSends {
    fn default() -> Self {
        Self(Mutex::new(Some(JoinSet::new())))
    }
}

impl BackgroundSends {
    /// Runs `task` on the current runtime. `false` (task dropped, never run) after `close`.
    pub(crate) fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) -> bool {
        let mut g = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let Some(set) = g.as_mut() else {
            return false;
        };
        // Reap finished sends so the set only holds running ones.
        while let Some(r) = set.try_join_next() {
            log_panic(r);
        }
        set.spawn(task);
        true
    }

    /// Aborts every running send and waits until they are gone. Idempotent.
    pub(crate) async fn close(&self) {
        let set = self.0.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(mut set) = set {
            set.abort_all();
            while let Some(r) = set.join_next().await {
                log_panic(r);
            }
        }
    }
}

/// A send that panicked skipped its own logging and unoffer: say so (aborts at close are normal).
fn log_panic(r: Result<(), JoinError>) {
    if let Err(e) = r
        && e.is_panic()
    {
        tracing::warn!("background clip send panicked: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn close_aborts_running_sends_and_refuses_new_ones() {
        let b = BackgroundSends::default();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        assert!(b.spawn(async move {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            let _ = tx.send(());
        }));
        tokio::time::timeout(Duration::from_secs(5), b.close())
            .await
            .expect("close must not wait for the send to finish");
        assert!(rx.await.is_err(), "aborted send dropped its sender");
        assert!(!b.spawn(async {}));
        b.close().await;
    }

    #[tokio::test]
    async fn a_panicking_send_is_reaped_and_does_not_poison_the_set() {
        let b = BackgroundSends::default();
        assert!(b.spawn(async { panic!("send bug") }));
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(
            b.spawn(async {}),
            "reaping the panicked task (logged) keeps spawning"
        );
        b.close().await;
    }
}
