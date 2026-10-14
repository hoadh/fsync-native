//! start / stop of the engine.

use super::{
    FsyncService,
    engine_release::{
        INLINE_RELEASE_WAIT, LINGERING_LOCK_MAX_HOLD, POLL_INLINE, POLL_LINGERING,
        hold_lock_until_released, wait_unique,
    },
    error::{FsyncError, SvcResult},
    inner::{Inner, lock},
};
use crate::{config::EngineConfig, engine::Engine, net::Node};
use std::sync::Arc;

impl Inner {
    pub(super) async fn start(self: &Arc<Self>) -> SvcResult<()> {
        let _one_start = self.start_gate.lock().await;
        if lock(&self.live)?.engine.is_some() {
            return Ok(());
        }
        let data_lock = self.lock_data_dir()?;
        let settings = self.history.settings().await?;
        let c = &self.cfg;
        let mut b = Node::builder(self.identity.clone(), self.trust.clone(), self.sink.clone())
            .mdns(c.mdns && !c.force_relay)
            .relay_only(c.force_relay);
        if let Some(r) = &c.relay_url {
            b = b.relays(vec![r.clone()]);
        }
        let ecfg = EngineConfig {
            max_clip_bytes: settings.max_clip_bytes,
            ..EngineConfig::default()
        };
        let engine =
            Engine::start_with_history(b, self.store.clone(), ecfg, Some(self.history.clone()))
                .await?;
        {
            let mut live = lock(&self.live)?;
            live.engine = Some(Arc::new(engine));
            live.data_lock = Some(data_lock);
        }
        self.restart_presence()
    }

    /// Two engines on one data dir would split incoming clips and fight over history: only one
    /// may run. The lock dies with the process, so a crash never leaves the dir blocked.
    fn lock_data_dir(&self) -> SvcResult<std::fs::File> {
        let path = self.cfg.data_dir.join("engine.lock");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|e| FsyncError::Io(format!("open {}: {e}", path.display())))?;
        match file.try_lock() {
            Ok(()) => Ok(file),
            Err(std::fs::TryLockError::WouldBlock) => Err(FsyncError::invalid(
                "another fsync engine is already running on this data directory",
            )),
            Err(std::fs::TryLockError::Error(e)) => {
                Err(FsyncError::Io(format!("lock {}: {e}", path.display())))
            }
        }
    }

    pub(super) async fn stop(&self) {
        let _one_start = self.start_gate.lock().await;
        let (engine, _presence, tasks, data_lock) = match lock(&self.live) {
            Ok(mut l) => (
                l.engine.take(),
                l.presence.take(),
                std::mem::take(&mut l.tasks),
                l.data_lock.take(),
            ),
            Err(_) => return,
        };
        drop(tasks);
        if let Ok(mut p) = lock(&self.pairing) {
            *p = Default::default();
        }
        if let Ok(mut x) = lock(&self.xfers) {
            x.abort_all();
        }
        drop(_presence);
        let Some(engine) = engine else { return };
        // Aborted tasks release their engine handles shortly after.
        let engine = match wait_unique(engine, POLL_INLINE, INLINE_RELEASE_WAIT).await {
            Ok(e) => {
                e.shutdown().await;
                drop(data_lock);
                return;
            }
            Err(still_shared) => still_shared,
        };
        // Something still holds the engine. Keep the data-dir lock until it is really gone (then
        // shut down cleanly), so a new engine never overlaps with a lingering one, but only for
        // a bounded time: a leaked handle must not block restart forever.
        tracing::warn!("engine still referenced at stop; shutting down once released");
        tokio::spawn(hold_lock_until_released(
            engine,
            data_lock,
            POLL_LINGERING,
            LINGERING_LOCK_MAX_HOLD,
            |e: Engine| e.shutdown(),
        ));
    }
}

impl FsyncService {
    /// Brings the node online (LAN discovery, relay) and, when signed in and paired, presence.
    pub fn start(&self) -> SvcResult<()> {
        self.run(self.inner.start())
    }

    /// Closes connections and the node. `start` may be called again.
    pub fn stop(&self) -> SvcResult<()> {
        self.run(async {
            self.inner.stop().await;
            Ok(())
        })
    }

    /// This device's node id (64 hex).
    pub fn node_id(&self) -> String {
        self.inner.me().to_string()
    }
}
