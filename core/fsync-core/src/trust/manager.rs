//! `TrustManager`: pins the highest verified device list; rejects rollback; gates trust resets.

use super::store::{PinnedState, TrustStore};
use crate::{
    error::{CoreError, Result},
    events::{EngineEvent, EventSink},
};
use fsync_proto::{DeviceList, NodeId, SignedDeviceList};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::watch;

/// `root_hash` of a pin made from the head alone (genesis never seen).
pub const UNKNOWN_ROOT: [u8; 32] = [0; 32];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ingest {
    Unchanged,
    Advanced {
        seq: u64,
    },
    /// Valid new root for the same account; nothing pinned until `confirm_reset(head_hash)`.
    /// `head_hash` identifies the offered head, i.e. exactly what was announced.
    ResetPending {
        head_hash: [u8; 32],
    },
}

/// Declined roots remembered. Only user answers add to it, so junk roots cannot evict it.
pub(super) const MAX_DECLINED: usize = 32;

pub(super) struct Pinned {
    pub(super) state: PinnedState,
    pub(super) list: DeviceList,
}

#[derive(Default)]
pub(super) struct Inner {
    pub(super) pinned: Option<Pinned>,
    /// (root hash, latest head) of the one offered new chain, which is what was last announced.
    /// A different root replaces it, so there is at most one entry.
    pub(super) pending: Vec<([u8; 32], SignedDeviceList)>,
    /// Roots the user declined with the last head seen for each: later syncs neither list nor
    /// announce them again. Oldest first, at most `MAX_DECLINED`.
    pub(super) declined: Vec<([u8; 32], [u8; 32])>,
    /// This device is re-creating the account's root: its own reset is not a candidate.
    pub(super) resetting: u32,
    /// This device; set by the node. Used to detect removal from the list.
    pub(super) me: Option<NodeId>,
    /// Pinned list no longer contains `me`: trust nobody until signed in / re-paired.
    pub(super) removed: bool,
}

pub struct TrustManager {
    pub(super) store: Arc<dyn TrustStore>,
    pub(super) inner: Mutex<Inner>,
    /// Serialises mutations so `inner` is never held across disk I/O.
    pub(super) gate: Mutex<()>,
    pub(super) events: EventSink,
    pub(super) version: watch::Sender<u64>,
}

impl TrustManager {
    pub fn new(store: Arc<dyn TrustStore>, events: EventSink) -> Result<Self> {
        let pinned = match store.load()? {
            Some(state) => {
                let list = state.head.decode()?;
                Some(Pinned { state, list })
            }
            None => None,
        };
        Ok(Self {
            store,
            inner: Mutex::new(Inner {
                pinned,
                ..Default::default()
            }),
            gate: Mutex::new(()),
            events,
            version: watch::channel(0).0,
        })
    }

    pub(super) fn lock(&self) -> Result<MutexGuard<'_, Inner>> {
        self.inner
            .lock()
            .map_err(|_| CoreError::Store("trust lock poisoned".into()))
    }

    pub(super) fn write_gate(&self) -> Result<MutexGuard<'_, ()>> {
        self.gate
            .lock()
            .map_err(|_| CoreError::Store("trust gate poisoned".into()))
    }

    /// Bumped on every pin change; the node closes connections of removed devices.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version.subscribe()
    }

    pub fn head(&self) -> Option<SignedDeviceList> {
        self.lock()
            .ok()?
            .pinned
            .as_ref()
            .map(|p| p.state.head.clone())
    }

    pub fn list(&self) -> Option<DeviceList> {
        self.lock().ok()?.pinned.as_ref().map(|p| p.list.clone())
    }

    pub fn root_hash(&self) -> Option<[u8; 32]> {
        self.lock().ok()?.pinned.as_ref().map(|p| p.state.root_hash)
    }

    /// Fail closed: lock errors or no pinned list mean "not trusted".
    pub fn is_trusted(&self, id: &NodeId) -> bool {
        self.lock()
            .ok()
            .and_then(|g| {
                (!g.removed)
                    .then(|| g.pinned.as_ref().map(|p| p.list.contains(id)))
                    .flatten()
            })
            .unwrap_or(false)
    }

    /// Root hashes of pending resets (ids shown to users are head hashes, see `pending_reset_infos`).
    pub fn pending_reset_roots(&self) -> Vec<[u8; 32]> {
        self.lock()
            .map(|g| g.pending.iter().map(|(h, _)| *h).collect())
            .unwrap_or_default()
    }

    /// Persist, then swap in memory. The caller holds `write_gate` and no `inner` guard, so
    /// readers are never blocked by the disk write.
    pub(super) fn pin(&self, head: SignedDeviceList, root_hash: [u8; 32]) -> Result<u64> {
        let list = head.decode()?;
        let state = PinnedState { head, root_hash };
        self.store.save(&state)?; // persist first: a crash never loses a newer pin
        let seq = list.seq;
        let mut g = self.lock()?;
        let gone = g.me.as_ref().is_some_and(|me| !list.contains(me));
        g.pinned = Some(Pinned { state, list });
        self.flag_removed(&mut g, gone);
        self.version.send_modify(|v| *v += 1);
        Ok(seq)
    }

    fn flag_removed(&self, g: &mut Inner, gone: bool) {
        if gone && !g.removed {
            self.events.emit(EngineEvent::SelfRemoved);
        }
        g.removed = gone;
    }

    /// Tell the manager which device this is. If the pinned list already lacks us (removed
    /// while offline), trust is revoked and `EngineEvent::SelfRemoved` fires.
    pub fn set_self(&self, me: NodeId) -> Result<()> {
        let mut g = self.lock()?;
        let gone = g.pinned.as_ref().is_some_and(|p| !p.list.contains(&me));
        g.me = Some(me);
        self.flag_removed(&mut g, gone);
        self.version.send_modify(|v| *v += 1);
        Ok(())
    }

    /// True once a pinned list dropped this device.
    pub fn is_self_removed(&self) -> bool {
        self.lock().map(|g| g.removed).unwrap_or(false) // `is_trusted` fails closed anyway
    }

    /// Sign-out: forget the pin.
    pub fn clear(&self) -> Result<()> {
        let _w = self.write_gate()?;
        self.store.clear()?;
        let mut g = self.lock()?;
        let me = g.me;
        *g = Inner {
            me,
            ..Inner::default()
        };
        self.version.send_modify(|v| *v += 1);
        Ok(())
    }
}
