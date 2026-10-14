//! Events surfaced to the app (phase 08 adapts this to the FFI delegate).

use fsync_proto::{ClipOffer, NodeId, Platform, pairing::PairMode};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

/// Connectivity to one peer: `Lan` = direct IP path (LAN or hole-punched), `Relay` = via relay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PathState {
    Offline,
    Lan,
    Relay,
}

/// One-shot user decision for the existing device; cloneable so the event can be.
#[derive(Clone)]
pub struct PairDecision(Arc<Mutex<Option<oneshot::Sender<bool>>>>);

impl PairDecision {
    pub fn channel() -> (Self, oneshot::Receiver<bool>) {
        let (tx, rx) = oneshot::channel();
        (Self(Arc::new(Mutex::new(Some(tx)))), rx)
    }
    fn answer(&self, v: bool) {
        if let Ok(mut g) = self.0.lock()
            && let Some(tx) = g.take()
        {
            let _ = tx.send(v);
        }
    }
    pub fn accept(&self) {
        self.answer(true)
    }
    pub fn reject(&self) {
        self.answer(false)
    }
}

impl std::fmt::Debug for PairDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PairDecision")
    }
}

#[derive(Clone, Debug)]
pub enum PairingEvent {
    /// Both devices: "Pair with <name>?". Compare `sas` with the other screen, then
    /// `accept()` / `reject()`. On the new device `peer`/`name` describe the existing device.
    /// No answer within two minutes counts as a rejection; nothing is pinned or published
    /// before both sides accept.
    ConfirmRequest {
        peer: NodeId,
        name: String,
        platform: Platform,
        mode: PairMode,
        sas: String,
        decision: PairDecision,
    },
    /// New list pinned (new device) or published (existing device).
    Completed {
        peer: NodeId,
        seq: u64,
    },
    Failed {
        reason: String,
    },
}

/// Shared, content-redacting view of a peer's validated `ClipOffer` (Debug never prints bytes).
#[derive(Clone)]
pub struct OfferRef(pub Arc<ClipOffer>);

impl std::ops::Deref for OfferRef {
    type Target = ClipOffer;
    fn deref(&self) -> &ClipOffer {
        &self.0
    }
}

impl std::fmt::Debug for OfferRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Offer({}, {:?})", self.0.item_id, self.0.kind)
    }
}

#[derive(Clone, Debug)]
pub enum EngineEvent {
    /// A paired device announced clipboard content (validated; deduped; echo guard armed).
    RemoteClip {
        from: NodeId,
        offer: OfferRef,
    },
    /// "Send to..." push of files (`offer.push`, kind Files). App asks / auto-accepts, then
    /// calls `Engine::fetch_file_to` per file.
    IncomingFiles {
        from: NodeId,
        offer: OfferRef,
    },
    /// A paired device pulled a file to its end (the stream was fully read by the receiver).
    FileServed {
        to: NodeId,
        item_id: uuid::Uuid,
        file_id: u32,
    },
    DeviceStatus {
        node_id: NodeId,
        state: PathState,
    },
    /// Pinned list advanced to `seq`.
    ListAdvanced {
        seq: u64,
    },
    Pairing(PairingEvent),
    /// A newly pinned list no longer contains this device: it stops trusting and accepting
    /// anyone. The app should sign out / wipe local data.
    SelfRemoved,
    /// Same account, new genesis root. Needs explicit `TrustManager::confirm_reset(head_hash)`;
    /// announced again (new `head_hash`) when the offered chain grows.
    TrustReset {
        head_hash: [u8; 32],
        device_names: Vec<String>,
    },
    Error {
        message: String,
    },
    /// The history database could not be opened with the current key (key lost) or was
    /// corrupt: it was moved to `backup` and a fresh one started. Pinned trust state lived in
    /// the same database, so a paired device may have to pair again.
    HistoryReset {
        backup: std::path::PathBuf,
    },
}

/// Cheap clonable sender; dropped events (full / closed receiver) are ignored.
#[derive(Clone)]
pub struct EventSink(mpsc::Sender<EngineEvent>);

pub const EVENT_BUFFER: usize = 256;

impl EventSink {
    pub fn channel() -> (Self, mpsc::Receiver<EngineEvent>) {
        let (tx, rx) = mpsc::channel(EVENT_BUFFER);
        (Self(tx), rx)
    }
    pub fn emit(&self, e: EngineEvent) {
        if self.0.try_send(e).is_err() {
            tracing::warn!("engine event dropped (receiver slow or closed)");
        }
    }
    pub fn pairing(&self, e: PairingEvent) {
        self.emit(EngineEvent::Pairing(e));
    }
    pub fn error(&self, message: impl Into<String>) {
        self.emit(EngineEvent::Error {
            message: message.into(),
        });
    }
}
