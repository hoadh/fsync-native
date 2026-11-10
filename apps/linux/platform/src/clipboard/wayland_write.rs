//! Writing side of `WlState`: our own source (create, replace, cancel, close).

use super::wayland_loop::Cmd;
use super::wayland_watch::WlState;
use super::{OwnedEntry, WriteError};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

impl WlState {
    pub(super) fn drop_source(&mut self) {
        if let Some(s) = self.source.take() {
            s.destroy();
        }
        self.entry = None;
        self.shared.owned.store(false, Ordering::SeqCst);
    }

    pub(super) fn write(&mut self, entry: Arc<OwnedEntry>) -> Result<(), WriteError> {
        let source = self.manager.create_source(&self.qh);
        for name in entry.offered_names() {
            source.offer(&name);
        }
        self.device.set_selection(Some(&source));
        if let Some(old) = self.source.replace(source) {
            old.destroy();
        }
        self.entry = Some(entry);
        self.shared.owned.store(true, Ordering::SeqCst);
        self.conn
            .flush()
            .map_err(|e| WriteError::Failed(e.to_string()))
    }

    pub(super) fn command(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Write(entry, reply) => {
                let _ = reply.send(self.write(entry));
            }
        }
    }

    /// Stops serving: destroys our source (which also clears the selection if it is ours).
    pub(super) fn close(&mut self) {
        self.drop_source();
        if let Some(c) = self.current.take() {
            c.retire();
        }
        self.device.destroy();
        let _ = self.conn.flush();
        // In-flight pastes see the stop flag within one poll slice; the shutdown budget is 1 s.
        if !self.writers.wait_idle(Duration::from_millis(500)) {
            tracing::warn!("clipboard: a paste transfer is still running at shutdown");
        }
    }
}
