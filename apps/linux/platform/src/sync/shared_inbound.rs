//! Inbound half of `Shared`: eager fetch and apply, and the watcher loop.

use super::inbound::{Fetched, fetch_all, kind_name};
use super::lanes::{Cancel, LaneHandle};
use super::shared::{OutJob, Shared, lock};
use super::status::PendingInfo;
use crate::clipboard::echo::is_own;
use crate::clipboard::{ClipChange, OwnedEntry, WriteError};
use fsync_core::service::ClipItem;
use fsync_proto::ClipKind;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Who and what, for the status line and the pending slot.
pub(super) fn pending_info(item: &ClipItem) -> PendingInfo {
    PendingInfo {
        kind: kind_name(item.kind).to_owned(),
        size: item.total_size,
        from_name: if item.from_name.is_empty() {
            "another device".to_owned()
        } else {
            item.from_name.clone()
        },
    }
}

impl Shared {
    pub(super) fn run_in(&self, item: ClipItem, cancel: &Cancel) {
        let settings = self.service.settings();
        if !settings.active {
            return;
        }
        self.note_mode(&settings);
        if item.kind == ClipKind::Files {
            return self.run_files(&item, cancel);
        }
        let info = pending_info(&item);
        match fetch_all(self.service.as_ref(), &item, cancel) {
            Fetched::Entry(entry) => self.apply(entry, info),
            Fetched::Cancelled => {}
            Fetched::Unwritable => self
                .status
                .event("Received a clip that cannot be put on the clipboard."),
            Fetched::Failed(e) => self
                .status
                .event(format!("Could not receive the clip: {e}")),
        }
    }

    pub(super) fn auto(&self) -> bool {
        self.backend.is_some()
            && self.service.settings().auto
            && !self.paused.load(Ordering::SeqCst)
    }

    pub(super) fn apply(&self, entry: OwnedEntry, info: PendingInfo) {
        if self.auto() {
            match self.write(&entry) {
                Ok(()) => {
                    *lock(&self.pending) = None;
                    self.status.update_event(|s| {
                        s.pending = None;
                        s.last_event = format!("Received {} from {}.", info.kind, info.from_name);
                    });
                    return;
                }
                Err(WriteError::TooLargeForX11(n)) => {
                    return self.status.event(format!(
                        "Received a {n} byte clip that X11 cannot take; skipped."
                    ));
                }
                Err(e) => tracing::warn!("clipboard write failed, holding the clip: {e}"),
            }
        }
        *lock(&self.pending) = Some(entry);
        self.status.update_event(|s| {
            s.last_event = format!(
                "Received {} from {}: waiting for Paste.",
                info.kind, info.from_name
            );
            s.pending = Some(info);
        });
    }

    pub(super) fn write(&self, entry: &OwnedEntry) -> Result<(), WriteError> {
        let backend = self.backend.as_ref().ok_or(WriteError::Closed)?;
        self.forget_last_auto();
        self.guard.remember(entry);
        backend
            .write(entry.clone())
            .inspect_err(|_| self.guard.clear())
    }

    pub(super) fn watch(
        &self,
        rx: std::sync::mpsc::Receiver<ClipChange>,
        out: &LaneHandle<OutJob>,
    ) {
        use std::sync::mpsc::RecvTimeoutError::{Disconnected, Timeout};
        while !self.closed.load(Ordering::SeqCst) {
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(change) => {
                    if is_own(&change.mimes) {
                        self.guard.clear();
                        continue;
                    }
                    let settings = self.service.settings();
                    if !settings.active {
                        continue; // signed out: the clipboard is not even read
                    }
                    self.note_mode(&settings);
                    *lock(&self.last) = Some(change.clone());
                    if !self.paused.load(Ordering::SeqCst) && settings.auto {
                        out.submit(OutJob::Auto(change));
                    }
                }
                Err(Timeout) => {}
                Err(Disconnected) => {
                    if !self.closed.load(Ordering::SeqCst) {
                        tracing::warn!("clipboard backend stopped");
                        self.status.update_event(|s| {
                            s.backend_lost = true;
                            s.last_event =
                                "The clipboard connection was lost. Restart fsync.".into();
                        });
                    }
                    return;
                }
            }
        }
    }
}
