//! State shared by the clip/file handlers and the `Engine` API.

use crate::{
    blobs::BlobStore,
    clip::{EchoGuard, PeerRate, ReceivedLog},
    config::EngineConfig,
    error::{CoreError, Result},
    events::EventSink,
    file::{FetchLocks, Outbox, ReaderCache},
    history::History,
    net::Dialer,
    trust::TrustManager,
};
use fsync_proto::NodeId;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicU64, Ordering},
};

pub struct Shared {
    pub me: NodeId,
    pub trust: Arc<TrustManager>,
    pub events: EventSink,
    pub cfg: EngineConfig,
    /// Live clip size cap (starts at `cfg.max_clip_bytes`; settings may change it).
    pub(crate) max_clip: AtomicU64,
    pub store: Arc<dyn BlobStore>,
    pub history: Option<History>,
    pub outbox: Mutex<Outbox>,
    pub log: Mutex<ReceivedLog>,
    pub echo: EchoGuard,
    pub fetch_locks: FetchLocks,
    pub offer_rate: PeerRate,
    pub readers: Mutex<ReaderCache>,
    /// Set right after the node is spawned (handlers exist before the endpoint does).
    pub(crate) dialer: OnceLock<Dialer>,
}

impl Shared {
    pub fn max_clip_bytes(&self) -> u64 {
        self.max_clip.load(Ordering::Relaxed)
    }

    pub fn set_max_clip_bytes(&self, v: u64) {
        self.max_clip.store(v, Ordering::Relaxed);
    }

    pub fn dialer(&self) -> Result<&Dialer> {
        self.dialer.get().ok_or(CoreError::Closed)
    }

    /// Drop blobs nothing else uses: no live outbox entry serves them and no history item
    /// references them.
    pub(crate) fn remove_blobs(&self, hashes: Vec<[u8; 32]>) {
        let served = self
            .outbox
            .lock()
            .map(|o| o.blob_hashes())
            .unwrap_or_default();
        for h in hashes {
            if served.contains(&h) {
                continue;
            }
            // History may still show the item: its blob is not ours to delete.
            if let Some(hist) = &self.history
                && hist.blob_referenced_sync(&h).unwrap_or(true)
            {
                continue;
            }
            if let Err(e) = self.store.remove(&h) {
                tracing::debug!("spilled blob cleanup failed: {e}");
            }
        }
    }
}
