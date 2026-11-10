//! The History view: load / search, pin, delete, clear, copy-back and the large-clip download.
//! Every call runs on an op thread; results go into `core.files.history` and the model is poked.

use super::ops::spawn;
use super::{clipboard_runtime, inbox_runtime, user_messages};
use crate::core_state::Core;
use crate::core_state::files_state::{HistoryRow, lock};
use crate::core_state::gate::OpKind;
use fsync_core::service::HistoryItem;
use fsync_linux_platform::clipboard::formats::{files_entry, to_entry};
use fsync_linux_platform::files::inbox::FetchError;
use fsync_linux_platform::sync::Paste;
use fsync_proto::ClipKind;
use std::sync::Arc;

const PAGE: u32 = 200;

#[derive(Debug, PartialEq, Eq)]
pub enum CopyOutcome {
    Done,
    /// Handed to the GUI thread, which reports the result itself.
    Queued,
}

fn kind_label(k: ClipKind) -> &'static str {
    match k {
        ClipKind::Text => "text",
        ClipKind::Rich => "rich text",
        ClipKind::Image => "image",
        ClipKind::Files => "files",
    }
}

fn row_of(core: &Core, i: HistoryItem) -> HistoryRow {
    let files = i.kind == ClipKind::Files;
    let in_inbox = files && inbox_runtime::inbox(core).complete_entries(&i.id).is_some();
    let peer_name = if i.outgoing {
        "This device".to_owned()
    } else {
        i.peer
            .as_ref()
            .and_then(|p| {
                lock_devices(core)
                    .into_iter()
                    .find(|(id, _)| id == p)
                    .map(|(_, n)| n)
            })
            .unwrap_or_else(|| "Another device".to_owned())
    };
    HistoryRow {
        kind: kind_label(i.kind),
        preview: i.preview.unwrap_or_default(),
        peer_name,
        created_ms: i.created_ms,
        total_size: i.total_size,
        pinned: i.pinned,
        outgoing: i.outgoing,
        can_copy: !files || in_inbox,
        can_download: files && !i.outgoing && !in_inbox && core.files.is_large_held(&i.id),
        downloading: core.inbox.is_downloading(&i.id),
        item_id: i.id,
    }
}

fn lock_devices(core: &Core) -> Vec<(String, String)> {
    crate::core_state::lock(&core.devices)
        .iter()
        .map(|d| (d.node_id.clone(), d.name.clone()))
        .collect()
}

fn set_message(core: &Core, text: impl Into<String>) {
    lock(&core.files.history).message = text.into();
    core.hub.poke_history();
}

/// Reloads the list for the current query (a newer load makes an older one stale).
pub fn load_blocking(core: &Core) {
    let Some(svc) = core.service() else { return };
    let generation = core.gate.begin(OpKind::History);
    let query = lock(&core.files.history).query.clone();
    lock(&core.files.history).loading = true;
    core.hub.poke_history();
    let result = svc.history((!query.is_empty()).then_some(query), None, PAGE);
    if !core.gate.is_current(OpKind::History, generation) {
        return;
    }
    let mut h = lock(&core.files.history);
    h.loading = false;
    match result {
        Ok(page) => {
            drop(h);
            let rows: Vec<HistoryRow> = page.items.into_iter().map(|i| row_of(core, i)).collect();
            // Re-check under the lock: a sign-out while the rows were built must not refill them.
            let mut h = lock(&core.files.history);
            if !core.gate.is_current(OpKind::History, generation) {
                return;
            }
            h.rows = rows;
        }
        Err(e) => h.message = user_messages::for_error(&e),
    }
    core.hub.poke_history();
}

pub fn reload(core: &Arc<Core>) {
    spawn(core, "history-load", |c| load_blocking(c));
}

pub fn search(core: &Arc<Core>, query: String) {
    lock(&core.files.history).query = query.trim().to_owned();
    reload(core);
}

/// Pin, delete or clear, then reload.
fn mutate(
    core: &Arc<Core>,
    name: &'static str,
    f: impl FnOnce(&fsync_core::service::FsyncService) -> Result<(), fsync_core::service::FsyncError>
    + Send
    + 'static,
) {
    spawn(core, name, move |c| {
        if let Some(svc) = c.service() {
            match f(&svc) {
                Ok(()) => lock(&c.files.history).message.clear(),
                Err(e) => set_message(c, user_messages::for_error(&e)),
            }
        }
        load_blocking(c);
    });
}

pub fn pin(core: &Arc<Core>, item_id: String, pinned: bool) {
    mutate(core, "history-pin", move |s| s.pin(&item_id, pinned));
}

pub fn delete(core: &Arc<Core>, item_id: String) {
    mutate(core, "history-delete", move |s| s.delete(&item_id));
}

/// Clears the history but keeps pinned items.
pub fn clear_unpinned(core: &Arc<Core>) {
    mutate(core, "history-clear", |s| s.clear(true));
}

/// Copy-back: the item's full content (or its inbox files) back onto the clipboard.
pub fn copy_blocking(core: &Arc<Core>, item_id: &str) -> Result<CopyOutcome, String> {
    let svc = core.service().ok_or("The engine is not ready yet.")?;
    let engine = clipboard_runtime::engine(core).ok_or("The clipboard is not ready yet.")?;
    let (entry, files_item) =
        if let Some(paths) = inbox_runtime::inbox(core).complete_entries(item_id) {
            (files_entry(&paths), Some(item_id.to_owned()))
        } else {
            let reps = svc
                .history_item_reps(item_id)
                .map_err(|e| user_messages::for_error(&e))?;
            let entry = to_entry(&reps);
            if entry.reps.is_empty() {
                return Err("The files are no longer here, or this item cannot be copied.".into());
            }
            (entry, None)
        };
    match engine.copy_back(entry, files_item) {
        Paste::Done => Ok(CopyOutcome::Done),
        // No background backend (GNOME): the focused History window writes it.
        Paste::NeedsWindow(entry) => {
            if core.hub.write_from_window(entry) {
                Ok(CopyOutcome::Queued)
            } else {
                Err("The clipboard could not be written.".into())
            }
        }
        Paste::Failed(e) => Err(format!("Could not copy: {e}")),
        Paste::Nothing => Err("Nothing to copy.".into()),
    }
}

/// Copies back and reports how: `Done`, or `Queued` (the GUI thread writes and sets the message).
pub fn copy(core: &Arc<Core>, item_id: String) {
    spawn(core, "history-copy", move |c| {
        match copy_blocking(c, &item_id) {
            Ok(CopyOutcome::Done) => set_message(c, "Copied."),
            Ok(CopyOutcome::Queued) => {}
            Err(e) => set_message(c, e),
        }
    });
}

pub fn download(core: &Arc<Core>, item_id: String) {
    spawn(core, "history-download", move |c| {
        let guard = match c.inbox.begin_download(&item_id) {
            Ok(g) => g,
            Err(FetchError::Failed(text)) => return set_message(c, text),
            Err(FetchError::Cancelled) => return set_message(c, "Cancelled."),
        };
        set_message(c, "Downloading...");
        load_blocking(c); // the row shows as busy while the claim is held
        match inbox_runtime::download_blocking(c, &guard, &item_id, true) {
            Ok(()) => set_message(c, "Downloaded."),
            Err(e) => set_message(c, e),
        }
        drop(guard);
        load_blocking(c);
    });
}
