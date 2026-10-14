//! `Engine`: a `Node` plus the clip/1 and file/1 protocols and their local state.

mod api;
mod history_hooks;
mod place;
mod shared;

pub use shared::Shared;

use crate::{
    blobs::BlobStore,
    clip::{ClipHandler, EchoGuard},
    config::EngineConfig,
    error::Result,
    file::{FileHandler, Outbox},
    history::History,
    net::{Node, NodeBuilder},
};
use fsync_proto::{ALPN_CLIP, ALPN_FILE};
use std::sync::{Arc, Mutex};

pub struct Engine {
    node: Node,
    sh: Arc<Shared>,
    rt: tokio::runtime::Handle,
    _retention: Option<history_hooks::TaskGuard>,
}

impl Engine {
    /// Spawn the node (relays/mDNS as configured on `builder`) with clip/1 and file/1 attached.
    pub async fn start(
        builder: NodeBuilder,
        store: Arc<dyn BlobStore>,
        cfg: EngineConfig,
    ) -> Result<Self> {
        Self::start_with_history(builder, store, cfg, None).await
    }

    /// Same, recording sent and received items in `history` (whose blob store should be
    /// `store`) and running the retention job for it.
    pub async fn start_with_history(
        builder: NodeBuilder,
        store: Arc<dyn BlobStore>,
        cfg: EngineConfig,
        history: Option<History>,
    ) -> Result<Self> {
        let sh = Arc::new(Shared {
            me: builder.identity.node_id(),
            trust: builder.trust.clone(),
            events: builder.events.clone(),
            outbox: Mutex::new(Outbox::new(cfg.outbox_ttl, cfg.outbox_grace)),
            max_clip: std::sync::atomic::AtomicU64::new(cfg.max_clip_bytes),
            cfg,
            store,
            history: history.clone(),
            log: Mutex::default(),
            echo: EchoGuard::default(),
            fetch_locks: Default::default(),
            offer_rate: Default::default(),
            readers: Mutex::default(),
            dialer: Default::default(),
        });
        let node = builder
            .protocol(ALPN_CLIP, ClipHandler(sh.clone()))
            .protocol(ALPN_FILE, FileHandler(sh.clone()))
            .spawn()
            .await?;
        let _ = sh.dialer.set(node.dialer());
        let _retention = history.map(|h| history_hooks::start_retention(h, &sh));
        Ok(Self {
            node,
            sh,
            rt: tokio::runtime::Handle::current(),
            _retention,
        })
    }

    /// Pairing, discovery, path state.
    pub fn node(&self) -> &Node {
        &self.node
    }

    /// Change the clipboard size cap for items sent and accepted from now on.
    pub fn set_max_clip_bytes(&self, v: u64) {
        self.sh.set_max_clip_bytes(v);
    }

    pub async fn shutdown(self) {
        self.close().await;
    }

    /// Gracefully stops the node and background tasks through a shared reference, for an engine
    /// that a leaked handle keeps alive. Idempotent with `shutdown` and drop.
    pub async fn close(&self) {
        if let Some(r) = &self._retention {
            r.abort();
        }
        self.node.close().await;
    }
}
