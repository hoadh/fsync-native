//! The work of the QuickClipWindow, run on the GUI thread once the window has the focus:
//! read the clipboard through Qt and publish (send), or write the held clip through Qt (paste).
//! Never called from an unfocused surface: Qt would return stale or empty data and the result
//! would look like a success.

use super::clipboard_runtime::engine;
use crate::bridge::clip_access::{QtReader, formats, set_entry, settle};
use crate::core_state::Core;
use fsync_linux_platform::clipboard::{ClipChange, MARKER_MIME};
use fsync_linux_platform::sync::{Paste, SyncEngine};
use std::sync::Arc;

pub fn run(core: &Core, action: &str) -> String {
    let Some(engine) = engine(core) else {
        return "fsync is not ready yet.".into();
    };
    match action {
        "send" => send(&engine),
        "paste" => paste(&engine),
        _ => "Unknown action.".into(),
    }
}

fn send(engine: &SyncEngine) -> String {
    let mimes = formats();
    if mimes.is_empty() {
        engine.event("Nothing to send: the clipboard is empty.");
        return "The clipboard is empty.".into();
    }
    let change = ClipChange {
        mimes,
        source: None, // Qt cannot tell who copied
        reader: Arc::new(QtReader),
    };
    match engine.collect_local(&change) {
        Ok(clip) => {
            engine.publish_clip(clip);
            "Sending...".into()
        }
        Err(skip) => skip.user_text().unwrap_or("Not sent.").into(),
    }
}

/// On Wayland the local read-back right after `setMimeData` always shows our own source, so it
/// proves nothing. After a compositor round trip the marker is still there only if no other
/// client took the selection meanwhile (a cancellation replaces the offer).
fn confirmed_after_roundtrip() -> bool {
    settle(180);
    holds_our_entry()
}

fn holds_our_entry() -> bool {
    formats()
        .iter()
        .any(|m| m.eq_ignore_ascii_case(MARKER_MIME))
}

fn paste(engine: &SyncEngine) -> String {
    match engine.paste() {
        Paste::NeedsWindow(entry) => {
            // The held clip is dropped only once the clipboard visibly holds our entry (marker
            // read back); otherwise it stays pending and the user can try again.
            if set_entry(&entry) && confirmed_after_roundtrip() {
                engine.pasted();
                "Pasted. Press Ctrl+V.".into()
            } else {
                engine.event("Could not confirm the paste: the clip is still waiting.");
                "Could not write the clipboard.".into()
            }
        }
        Paste::Done => "Pasted. Press Ctrl+V.".into(),
        Paste::Nothing => "Nothing to paste.".into(),
        Paste::Failed(e) => format!("Could not paste: {e}"),
    }
}
