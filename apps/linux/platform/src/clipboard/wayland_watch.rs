//! Wayland connection state: watches the regular selection (MIME list first, bytes on demand
//! through `OfferReader`) and owns at most one source (our entry). Commands from other threads
//! arrive over a channel plus a wake socket, see `wayland_loop`.

use super::wayland_loop::Shared;
use super::wayland_proto::{Device, Manager, Offer, Source};
use super::wayland_reader::OfferReader;
use super::wayland_source::WriterGate;
use super::{ClipChange, OwnedEntry, echo::is_own, source_app_ipc};
use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use wayland_client::backend::ObjectId;
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::{Connection, Dispatch, QueueHandle};

pub struct WlState {
    pub(super) manager: Manager,
    pub(super) device: Device,
    pub(super) qh: QueueHandle<WlState>,
    pub(super) conn: Connection,
    pub(super) changes: Sender<ClipChange>,
    /// Announce changes (false while the initial selection is being received at start).
    pub(super) announce: bool,
    /// Captured at start (see `FocusEnv`).
    focus_env: source_app_ipc::FocusEnv,
    pending: HashMap<ObjectId, (Offer, Vec<String>)>,
    pub(super) current: Option<Arc<OfferReader>>,
    pub(super) source: Option<Source>,
    pub(super) entry: Option<Arc<OwnedEntry>>,
    pub(super) writers: Arc<WriterGate>,
    pub(super) stop: Arc<AtomicBool>,
    pub(super) shared: Arc<Shared>,
}

impl Dispatch<WlRegistry, GlobalListContents> for WlState {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as wayland_client::Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl WlState {
    pub(super) fn new(
        manager: Manager,
        device: Device,
        qh: QueueHandle<WlState>,
        conn: Connection,
        changes: Sender<ClipChange>,
        stop: Arc<AtomicBool>,
        shared: Arc<Shared>,
    ) -> Self {
        Self {
            manager,
            device,
            qh,
            conn,
            changes,
            stop,
            shared,
            announce: false,
            focus_env: source_app_ipc::FocusEnv::capture(&|k| std::env::var(k).ok()),
            pending: HashMap::new(),
            current: None,
            source: None,
            entry: None,
            writers: Arc::new(WriterGate::default()),
        }
    }

    pub(super) fn on_data_offer(&mut self, offer: Offer) {
        // Offers normally become a selection (or are destroyed as primary) at once; a pile-up
        // means ones that never will: drop them instead of leaking.
        if self.pending.len() >= 8 {
            for (_, (stale, _)) in self.pending.drain() {
                stale.destroy();
            }
        }
        self.pending.insert(offer.id(), (offer, Vec::new()));
    }

    pub(super) fn on_offer_mime(&mut self, id: ObjectId, mime: String) {
        if let Some((_, mimes)) = self.pending.get_mut(&id) {
            mimes.push(mime);
        }
    }

    /// Primary selection is ignored: its offers are destroyed unread.
    pub(super) fn on_primary(&mut self, offer: Option<Offer>) {
        if let Some(o) = offer {
            self.pending.remove(&o.id());
            o.destroy();
        }
    }

    pub(super) fn on_finished(&mut self) {
        self.shared.dead.store(true, Ordering::SeqCst);
    }

    pub(super) fn on_selection(&mut self, offer: Option<Offer>) {
        if let Some(old) = self.current.take() {
            old.retire();
        }
        let Some(offer) = offer else {
            return;
        };
        let Some((offer, mimes)) = self.pending.remove(&offer.id()) else {
            return;
        };
        let reader = Arc::new(OfferReader {
            offer: Mutex::new(Some(offer)),
            conn: self.conn.clone(),
        });
        self.current = Some(Arc::clone(&reader));
        if !self.announce {
            return;
        }
        // Our own entry needs no identity lookup: the marker is enough.
        let source = (!is_own(&mimes))
            .then(|| source_app_ipc::focused_app(&self.focus_env))
            .flatten();
        // A closed receiver means the sync engine stopped: nothing left to tell.
        let _ = self.changes.send(ClipChange {
            mimes,
            source,
            reader,
        });
    }

    pub(super) fn on_send(&mut self, source: ObjectId, mime: String, fd: OwnedFd) {
        if self.source.as_ref().map(Source::id) != Some(source) || self.stop.load(Ordering::SeqCst)
        {
            return; // not our current entry, or shutting down: dropping the fd closes it
        }
        let Some(rep) = self
            .entry
            .as_ref()
            .and_then(|e| e.reps.iter().position(|r| r.offered.contains(&mime)))
        else {
            return;
        };
        self.serve_request(rep, fd);
    }

    pub(super) fn on_cancelled(&mut self, source: ObjectId) {
        if self.source.as_ref().map(Source::id) == Some(source) {
            self.drop_source();
        }
    }
}
