//! start / stop of the engine.

use super::{
    FsyncService,
    error::SvcResult,
    inner::{Inner, lock},
};
use crate::{config::EngineConfig, engine::Engine, net::Node};
use std::{sync::Arc, time::Duration};

impl Inner {
    pub(super) async fn start(self: &Arc<Self>) -> SvcResult<()> {
        if lock(&self.live)?.engine.is_some() {
            return Ok(());
        }
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
        lock(&self.live)?.engine = Some(Arc::new(engine));
        self.restart_presence()
    }

    pub(super) async fn stop(&self) {
        let (engine, _presence, tasks) = match lock(&self.live) {
            Ok(mut l) => (
                l.engine.take(),
                l.presence.take(),
                std::mem::take(&mut l.tasks),
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
        let Some(mut engine) = engine else { return };
        // Aborted tasks release their engine handles shortly after.
        for _ in 0..50 {
            match Arc::try_unwrap(engine) {
                Ok(e) => {
                    e.shutdown().await;
                    return;
                }
                Err(still_shared) => {
                    engine = still_shared;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
        tracing::warn!("engine still referenced at stop; dropping without clean shutdown");
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
