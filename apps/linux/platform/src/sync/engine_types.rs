//! Public types of the sync engine.

use crate::clipboard::OwnedEntry;
use crate::clipboard::path_denylist::PathDenylist;
use crate::files::inbox::Inbox;
use fsync_core::service::ClipItem;
use std::sync::Arc;

/// Environment the app provides.
pub struct SyncEnv {
    pub paths: PathDenylist,
    /// Where received file clips go; without it they are only reported.
    pub files: Option<FilesEnv>,
}

/// Received file clips: the eager inbox, and what to do with a clip too large to fetch
/// without asking (the app notifies and keeps it for the History Download button). The hook
/// runs on the inbound lane and must not block.
pub struct FilesEnv {
    pub inbox: Inbox,
    pub on_large: LargeHook,
}

pub type LargeHook = Arc<dyn Fn(&ClipItem) + Send + Sync>;

#[derive(Debug, PartialEq, Eq)]
pub enum Paste {
    Done,
    Nothing,
    /// No backend can write in the background: the caller writes this through a focused window.
    NeedsWindow(OwnedEntry),
    Failed(String),
}
