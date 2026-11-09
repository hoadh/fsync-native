//! What the status page and the tray show. Plain data, no clipboard content: kinds, sizes and
//! sentences only.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PendingInfo {
    pub kind: String,
    pub size: u64,
    pub from_name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncStatus {
    /// "Wayland (data-control)", "X11" or "Manual (Send/Paste window)".
    pub backend: String,
    /// Why this backend (detection reason).
    pub reason: String,
    /// What the sensitive filter can see on this session.
    pub capability: String,
    /// Core `clip_mode` is Auto and the backend can watch.
    pub auto: bool,
    pub paused: bool,
    pub pending: Option<PendingInfo>,
    /// Last outcome sentence ("Sent to 1 device", "Not sent: ..."); never content.
    pub last_event: String,
    /// The backend died (connection lost).
    pub backend_lost: bool,
}

/// Shared status with a change notification (the app pokes the Qt thread from it).
#[derive(Clone)]
pub struct StatusCell {
    inner: Arc<Mutex<SyncStatus>>,
    /// Counts `last_event` writes (bumped under the `inner` lock): a slow outcome (a send that
    /// finishes late) must not overwrite a newer event.
    events: Arc<AtomicU64>,
    notify: Arc<dyn Fn() + Send + Sync>,
}

impl StatusCell {
    pub fn new(initial: SyncStatus, notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(initial)),
            events: Arc::new(AtomicU64::new(0)),
            notify,
        }
    }

    pub fn get(&self) -> SyncStatus {
        self.inner.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// `update` for a writer that sets `last_event`: always counts as an event, even when the
    /// text equals the previous one (a second "Received text from X." is news too).
    pub fn update_event(&self, f: impl FnOnce(&mut SyncStatus)) {
        {
            let mut s = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            f(&mut s);
            self.events.fetch_add(1, Ordering::SeqCst);
        }
        (self.notify)();
    }

    /// Any other change of `last_event` counts as an event, whichever writer made it (received clip,
    /// paste, lost connection): a slow send outcome must never overwrite one of them.
    pub fn update(&self, f: impl FnOnce(&mut SyncStatus)) {
        {
            let mut s = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            let before = s.last_event.clone();
            f(&mut s);
            if s.last_event != before {
                self.events.fetch_add(1, Ordering::SeqCst);
            }
        }
        (self.notify)();
    }

    pub fn event(&self, text: impl Into<String>) {
        let text = text.into();
        self.update_event(|s| s.last_event = text);
    }

    /// Number of events written so far; pair with `event_if_unchanged`.
    pub fn event_seq(&self) -> u64 {
        self.events.load(Ordering::SeqCst)
    }

    /// Writes the event only if none was written since `seq` was read (a newer event wins).
    pub fn event_if_unchanged(&self, seq: u64, text: impl Into<String>) {
        let text = text.into();
        self.update(|s| {
            if self.events.load(Ordering::SeqCst) == seq {
                s.last_event = text;
                self.events.fetch_add(1, Ordering::SeqCst);
            }
        });
    }
}
