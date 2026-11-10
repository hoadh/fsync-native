//! "Send to a device", second half: the picker's answer and the send itself. Staging is in
//! `send_stage`, the pure rules in `send_validate`. A path is sent only when it passes the
//! allowlist / denylist (ADR 014) at stage time and again right before core is called; what is
//! validated is exactly what core receives (canonical UTF-8 `String`s).

use super::clipboard_runtime::engine;
use super::ops::{service_or_notify, show_error, spawn};
pub use super::send_stage::{cancel_pending, on_cold_start, reset_for_sign_out, stage_bounded};
use super::send_stage::{deny, refuse, stage_and_show};
pub use super::send_validate::*;
use crate::core_state::{Core, lock};
use fsync_core::service::{TransferDirection, TransferEvent, TransferState};
use std::path::PathBuf;
use std::sync::Arc;

/// The picker answered. `shown`: the stage id the picker displayed (`None` only for the test
/// command). A click answers only the set it saw; if another set was staged since, the picker
/// comes back with the current one.
pub fn pick(core: &Arc<Core>, node_id: &str, shown: Option<u64>) -> Result<(), &'static str> {
    let staged = lock(&core.pending_send).clone();
    if staged.is_empty() {
        return Err(NOTHING_STAGED);
    }
    if shown.is_some_and(|id| id != staged.id()) {
        core.hub.show_section(None, "send");
        return Err(STALE);
    }
    let node = node_id.to_owned();
    spawn(core, "send-files", move |c| {
        // Staged minutes ago: check again that every path is still allowed (a symlink or a
        // folder swapped in meanwhile).
        let raw: Vec<PathBuf> = staged.paths.iter().map(PathBuf::from).collect();
        match validate(&deny(c), &raw) {
            Ok(paths) => send_to(c, node, paths, Some(staged.id())),
            Err(code) => {
                cancel_pending(c);
                refuse(c, code);
            }
        }
    });
    Ok(())
}

fn device_name(core: &Core, node_id: &str) -> String {
    lock(&core.devices)
        .iter()
        .find(|d| d.node_id == node_id)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "the device".into())
}

fn note(core: &Core, text: &str) {
    match engine(core) {
        Some(e) => e.event(text),
        None => core.update_ui(|u| u.message = text.into()),
    }
}

/// Core `send_files` (op thread: it expands folders and may wait for the peer). `staged_id`:
/// the staged set this send answers; it is dropped only after core accepted it. A failed send
/// keeps the set and brings the picker back, so the user can pick another device.
fn send_to(core: &Arc<Core>, node_id: String, paths: Vec<String>, staged_id: Option<u64>) {
    let Some(svc) = service_or_notify(core) else {
        return;
    };
    note(
        core,
        &format!("Sending to {}...", device_name(core, &node_id)),
    );
    match svc.send_files(&node_id, paths) {
        Ok(_) => {
            let mut staged = lock(&core.pending_send);
            if staged_id.is_some_and(|id| id == staged.id()) {
                staged.clear();
                drop(staged);
                core.hub.poke_send();
            }
        }
        Err(e) => {
            note(core, "Not sent.");
            show_error(core, &e);
            // Back to the picker only while its set is still the one that failed.
            if staged_id.is_some_and(|id| id == lock(&core.pending_send).id()) {
                core.hub.show_section(None, "send");
            }
        }
    }
}

/// Window drop (QML, GUI thread): everything that touches the file system runs on an op thread.
/// `onto`: a device row that took the drop (sent at once), else the DevicePicker opens.
pub fn dropped(core: &Arc<Core>, urls: String, onto: Option<String>) {
    let raw = match urls_to_paths(&urls) {
        Ok(raw) => raw,
        Err(code) => return refuse(core, code),
    };
    match onto {
        None => {
            if let Err(code) = stage_and_show(core, raw, None, None) {
                refuse(core, code);
            }
        }
        Some(node) => spawn(core, "drop-send", move |c| match validate(&deny(c), &raw) {
            Ok(paths) => send_to(c, node, paths, None),
            Err(code) => refuse(c, code),
        }),
    }
}

/// Status sentence for the end of a push (core Send events).
pub fn send_event_text(state: TransferState) -> Option<&'static str> {
    match state {
        TransferState::Done => Some("Files sent."),
        TransferState::Failed => Some("Sending failed."),
        TransferState::Cancelled => Some("Sending cancelled."),
        _ => None,
    }
}

pub fn on_send_event(core: &Core, ev: &TransferEvent) {
    if ev.direction != TransferDirection::Send {
        return;
    }
    if let Some(text) = send_event_text(ev.state) {
        note(core, text);
    }
}

#[cfg(test)]
#[path = "send_files_tests.rs"]
mod tests;
