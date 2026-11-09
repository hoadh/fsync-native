//! Clipboard backends and the pure rules around them: format map, sensitive filters, path
//! denylist, echo guard. No Qt: everything here is unit-testable without a display.
//!
//! A backend turns display-server events into `ClipChange` (MIME list first, bytes only on
//! demand through `ChangeReader`) and serves one `OwnedEntry` (eager bytes only) after a write.

pub mod denylist_builtin;
pub mod detect;
pub mod echo;
pub mod filters;
pub mod formats;
pub mod path_denylist;
pub mod source_app;
pub mod source_app_ipc;
pub mod uri_list;
pub mod wayland_backend;
mod wayland_dispatch;
mod wayland_loop;
mod wayland_proto;
mod wayland_reader;
mod wayland_source;
mod wayland_watch;
mod wayland_write;
pub mod x11_backend;
mod x11_conn;
mod x11_owner;
mod x11_read;
mod x11_watch;

use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;
pub use wayland_proto::Protocol;

/// Marker MIME on every entry we write: our own echo is recognised without reading content.
pub const MARKER_MIME: &str = "application/x-fsync-origin";
/// Password-manager hint (KDE/KeePassXC convention). Honoured on read, set on our own entries.
pub const KDE_HINT_MIME: &str = "x-kde-passwordManagerHint";
/// macOS convention some Linux password managers copy.
pub const CONCEALED_MIME: &str = "org.nspasteboard.ConcealedType";

/// The application that owned the clipboard (X11) or had the focus (Wayland, best effort).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceApp {
    pub pid: Option<u32>,
    /// `/proc/<pid>/comm` or the executable basename.
    pub name: Option<String>,
    /// WM_CLASS (instance and class) or the Wayland app id.
    pub classes: Vec<String>,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ReadError {
    #[error("larger than the size limit")]
    TooLarge,
    #[error("the source did not answer in time")]
    Timeout,
    #[error("the clipboard changed before it could be read")]
    Stale,
    #[error("clipboard read failed: {0}")]
    Io(String),
}

/// Reads the bytes of one change; valid until the next change replaces it.
pub trait ChangeReader: Send + Sync {
    fn read(&self, mime: &str, cap: usize, timeout: Duration) -> Result<Vec<u8>, ReadError>;
}

/// One selection change: what is offered, who probably made it, and a handle to read it.
#[derive(Clone)]
pub struct ClipChange {
    pub mimes: Vec<String>,
    pub source: Option<SourceApp>,
    pub reader: Arc<dyn ChangeReader>,
}

impl std::fmt::Debug for ClipChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // MIME names only, never content.
        f.debug_struct("ClipChange")
            .field("mimes", &self.mimes)
            .field("source", &self.source)
            .finish()
    }
}

/// One representation we offer: bytes under every name in `offered` (text has aliases).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryRep {
    /// The wire MIME of the clip rep it came from (or the marker / hint name).
    pub mime: String,
    pub offered: Vec<String>,
    pub data: Vec<u8>,
}

impl EntryRep {
    /// The marker and the privacy hint are ours, not clip content.
    pub fn is_internal(&self) -> bool {
        self.mime == MARKER_MIME || self.mime == KDE_HINT_MIME
    }
}

/// What we put on the clipboard: every rep is ready bytes (inbound is always eager).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnedEntry {
    pub reps: Vec<EntryRep>,
}

impl OwnedEntry {
    pub fn total_bytes(&self) -> usize {
        self.reps.iter().map(|r| r.data.len()).sum()
    }

    /// Bytes served for `name`, with the rep that owns it.
    pub fn find(&self, name: &str) -> Option<&EntryRep> {
        self.reps
            .iter()
            .find(|r| r.offered.iter().any(|o| o == name))
    }

    /// Every offered name, in rep order, de-duplicated.
    pub fn offered_names(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for n in self.reps.iter().flat_map(|r| r.offered.iter()) {
            if !out.contains(n) {
                out.push(n.clone());
            }
        }
        out
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum WriteError {
    #[error("the clipboard backend is shutting down")]
    Closed,
    #[error("too large for this X11 server ({0} bytes), skipped")]
    TooLargeForX11(usize),
    #[error("clipboard write failed: {0}")]
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendKind {
    Wayland,
    X11,
}

impl BackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wayland => "Wayland (data-control)",
            Self::X11 => "X11",
        }
    }
}

pub trait Backend: Send + Sync {
    fn kind(&self) -> BackendKind;
    /// Changes of the selection, in order. Taken once by the sync engine.
    fn take_changes(&self) -> Option<Receiver<ClipChange>>;
    /// Replaces the clipboard with `entry` and keeps serving it while we own the selection.
    fn write(&self, entry: OwnedEntry) -> Result<(), WriteError>;
    /// Whether our entry is still the clipboard.
    fn owned(&self) -> bool;
    /// Stops watching and serving; returns within about 1 s.
    fn shutdown(&self);
}
