//! `SyncEngine`: the watcher thread feeds the outbound lane, `on_remote_clip` feeds the inbound
//! lane. Manual mode (core `clip_mode`, no backend, or paused) holds received clips as pending
//! and publishes only on an explicit send.

use super::engine_types::{Paste, SyncEnv};
use super::lanes::Lane;
use super::shared::{OutJob, Shared, lock};
use super::status::{StatusCell, SyncStatus};
use super::{ClipService, SyncSettings};
use crate::clipboard::echo::WriteGuard;
use crate::clipboard::filters::Skip;
use crate::clipboard::{Backend, ClipChange, OwnedEntry};
use fsync_core::service::{ClipItem, LocalClip};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub struct SyncEngine {
    sh: Arc<Shared>,
    out: Lane<OutJob>,
    inb: Lane<ClipItem>,
    watcher: Mutex<Option<JoinHandle<()>>>,
}

impl SyncEngine {
    pub fn start(
        service: Arc<dyn ClipService>,
        backend: Option<Arc<dyn Backend>>,
        env: SyncEnv,
        initial: SyncStatus,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> std::io::Result<Arc<Self>> {
        let sh = Arc::new(Shared {
            service,
            backend,
            env,
            guard: WriteGuard::default(),
            status: StatusCell::new(initial, notify),
            pending: Mutex::new(None),
            last: Mutex::new(None),
            current_files: Mutex::new(None),
            paused: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            last_auto: Mutex::new(None),
        });
        let s1 = Arc::clone(&sh);
        let out = Lane::spawn("out", move |job, cancel| s1.run_out(job, &cancel))?;
        let s2 = Arc::clone(&sh);
        let inb = Lane::spawn("in", move |item, cancel| s2.run_in(item, &cancel))?;
        let mut watcher = None;
        if let Some(rx) = sh.backend.as_ref().and_then(|b| b.take_changes()) {
            let (s3, to_out) = (Arc::clone(&sh), out.handle());
            watcher = Some(
                std::thread::Builder::new()
                    .name("fsync-clip-watch".into())
                    .spawn(move || s3.watch(rx, &to_out))?,
            );
        }
        Ok(Arc::new(Self {
            sh,
            out,
            inb,
            watcher: Mutex::new(watcher),
        }))
    }

    pub fn status(&self) -> SyncStatus {
        self.sh.status.get()
    }

    pub fn has_backend(&self) -> bool {
        self.sh.backend.is_some()
    }

    /// From core's callback thread: queues the clip, never blocks.
    pub fn on_remote_clip(&self, item: ClipItem) {
        if !self.sh.closed.load(Ordering::SeqCst) {
            self.inb.submit(item);
        }
    }

    pub fn set_paused(&self, paused: bool) {
        self.sh.paused.store(paused, Ordering::SeqCst);
        self.sh.status.update(|s| s.paused = paused);
    }

    pub fn paused(&self) -> bool {
        self.sh.paused.load(Ordering::SeqCst)
    }

    /// Explicit send of the latest clipboard change (Manual mode, tray, hotkey).
    pub fn send_now(&self) -> bool {
        let Some(change) = lock(&self.sh.last).clone() else {
            self.sh
                .status
                .event("Nothing to send yet: copy something first.");
            return false;
        };
        self.out.submit(OutJob::Explicit(change));
        true
    }

    /// Filters and reads a change on the calling thread (the GNOME window path, where the
    /// reader is only valid on the Qt thread); publish the result with `publish_clip`.
    pub fn collect_local(&self, change: &ClipChange) -> Result<LocalClip, Skip> {
        let r = self.sh.collect_local(change);
        if let Err(skip) = &r {
            self.sh.skipped(skip, true);
        }
        r
    }

    pub fn publish_clip(&self, clip: LocalClip) {
        self.out.submit(OutJob::Clip(clip));
    }

    /// Re-reads core's mode into the status (call after the setting changed).
    pub fn refresh_mode(&self) {
        self.sh.note_mode(&self.sh.service.settings());
    }

    pub fn settings(&self) -> SyncSettings {
        self.sh.service.settings()
    }

    /// Writes the held clip through the backend, or hands it to the caller when there is none.
    pub fn paste(&self) -> Paste {
        let Some(entry) = lock(&self.sh.pending).clone() else {
            self.sh.status.event("Nothing to paste.");
            return Paste::Nothing;
        };
        if self.sh.backend.is_none() {
            return Paste::NeedsWindow(entry);
        }
        match self.sh.write(&entry) {
            Ok(()) => {
                self.pasted();
                Paste::Done
            }
            Err(e) => {
                self.sh.status.event(format!("Could not paste: {e}"));
                Paste::Failed(e.to_string())
            }
        }
    }

    /// Puts `entry` (history copy-back) on the clipboard through the backend, or hands it to the
    /// caller when there is none (the caller then writes it from a focused window). `files_item`
    /// is the inbox item an entry of file references belongs to.
    pub fn copy_back(&self, entry: OwnedEntry, files_item: Option<String>) -> Paste {
        if let Some(id) = files_item {
            *lock(&self.sh.current_files) = Some(id);
        }
        if self.sh.backend.is_none() {
            return Paste::NeedsWindow(entry);
        }
        match self.sh.write(&entry) {
            Ok(()) => Paste::Done,
            Err(e) => Paste::Failed(e.to_string()),
        }
    }

    /// The inbox item currently on the clipboard (retention never removes it).
    pub fn current_file_item(&self) -> Option<String> {
        lock(&self.sh.current_files).clone()
    }

    /// The held clip was written by the caller (window path): clear it.
    pub fn pasted(&self) {
        *lock(&self.sh.pending) = None;
        self.sh.status.update_event(|s| {
            s.pending = None;
            s.last_event = "Pasted the received clip.".into();
        });
    }

    pub fn event(&self, text: &str) {
        self.sh.status.event(text);
    }

    /// Stops watching and serving, within `budget`: new work is refused first, then the backend
    /// stops serving (in-flight pastes finish or are cut within ~1 s), then the lanes drain.
    pub fn shutdown(&self, budget: Duration) {
        let deadline = Instant::now() + budget;
        self.sh.closed.store(true, Ordering::SeqCst);
        let left = || {
            deadline
                .saturating_duration_since(Instant::now())
                .max(Duration::from_millis(50))
        };
        // The backend (<= ~1 s) and the lanes stop side by side. A publish already running
        // cannot be cancelled inside core: it is awaited for the rest of the budget, so it has
        // returned (and released its service handle) before core `stop()` in the normal case.
        std::thread::scope(|s| {
            if let Some(b) = &self.sh.backend {
                s.spawn(|| b.shutdown());
            }
            if !self.out.shutdown(left()) {
                tracing::warn!("clipboard: a publish was still running at shutdown");
            }
            if !self.inb.shutdown(left()) {
                tracing::warn!("clipboard: a fetch was still running at shutdown");
            }
        });
        if let Some(t) = lock(&self.watcher).take() {
            let _ = t.join();
        }
        *lock(&self.sh.pending) = None;
        *lock(&self.sh.last) = None;
    }
}
