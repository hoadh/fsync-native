//! State shared by the clip/file handlers and the `Engine` API.

use crate::{
    blobs::BlobStore,
    clip::{BackgroundSends, EchoGuard, PeerRate, PeerSendLanes, ReceivedLog, SendJob},
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
    atomic::{AtomicU64, AtomicUsize, Ordering},
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
    /// Clip sends to one peer run in order, latest wins (see `PeerSendLanes`).
    pub(crate) send_lanes: PeerSendLanes<SendJob>,
    /// `publish` calls running now; more than one is logged (see `Shared::publish`).
    pub(crate) publishing: AtomicUsize,
    /// Test hook: every clip frame built, to count those still held by sends.
    #[cfg(feature = "test-hooks")]
    pub(crate) frames: Mutex<Vec<std::sync::Weak<[u8]>>>,
    /// Sends still running after `publish` returned; aborted by `Engine::close`.
    pub(crate) background: BackgroundSends,
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

    /// Test hook: remember `frame` so `clip_frames_held` can tell whether a send still holds it.
    #[cfg(feature = "test-hooks")]
    pub(crate) fn track_frame(&self, frame: &std::sync::Arc<[u8]>) {
        let mut f = self.frames.lock().unwrap_or_else(|p| p.into_inner());
        f.retain(|w| w.strong_count() > 0);
        f.push(std::sync::Arc::downgrade(frame));
    }

    /// Test hook: clip frames still alive (held by a running send or a clip waiting in a lane).
    #[cfg(feature = "test-hooks")]
    pub(crate) fn clip_frames_held(&self) -> usize {
        let f = self.frames.lock().unwrap_or_else(|p| p.into_inner());
        f.iter().filter(|w| w.strong_count() > 0).count()
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
