//! Test-only IPC commands (feature `e2e-control`, refused in release builds by the guard in
//! main.rs). `pair-confirm` is a SAS bypass, which is why this never ships.

use super::{
    clipboard_runtime, history_ops, inbox_runtime, offers, ops, ops_pairing, ops_session,
    send_files, settings_ops,
};
use crate::core_state::ui_state::EngineState;
use crate::core_state::{Core, lock};
use fsync_core::service::PairingMode;
use fsync_linux_platform::ipc::{E2eCommand, Reply};
use serde_json::json;
use std::sync::Arc;

/// Newest history items (previews are clip content: this exists for tests only).
fn history_json(core: &Core) -> Vec<serde_json::Value> {
    let Some(page) = core.service().and_then(|s| s.history(None, None, 50).ok()) else {
        return Vec::new();
    };
    page.items
        .into_iter()
        .map(|i| {
            json!({"id": i.id, "kind": format!("{:?}", i.kind), "preview": i.preview,
                   "outgoing": i.outgoing, "total_size": i.total_size, "pinned": i.pinned})
        })
        .collect()
}

fn status(core: &Core) -> Reply {
    let ui = lock(&core.ui).clone();
    let clip = clipboard_runtime::snapshot(core);
    let p = lock(&core.pairing).clone();
    let devices: Vec<_> = lock(&core.devices)
        .iter()
        .map(|d| {
            json!({"node_id": d.node_id, "name": d.name, "platform": d.platform,
                   "status": d.status, "is_self": d.is_self})
        })
        .collect();
    Reply::ok_with(json!({
        "state": ui.state.as_str(), "email": ui.email, "node_id": ui.node_id,
        "banner": ui.warnings.banner(ui.state == EngineState::Starting), "message": ui.message, "busy": ui.busy,
        "pairing": {"phase": p.phase.as_str(), "code": p.code, "sas": p.sas,
                    "peer_name": p.peer_name, "request_id": p.request_id.to_string(),
                    "message": p.message},
        "devices": devices,
        // Peers the backend reports online (a transport may not exist before the first send).
        "online_peers": core.service().map(|s| s.devices().into_iter()
            .filter(|d| !d.is_self && d.online).map(|d| d.name).collect::<Vec<_>>()).unwrap_or_default(),
        "offers": offers::snapshot_json(core),
        "history": history_json(core),
        "clip": {"backend": clip.backend, "reason": clip.reason, "capability": clip.capability,
                 "auto": clip.auto, "paused": clip.paused, "pending": clip.pending,
                 "last_event": clip.last_event},
    }))
}

pub fn handle(core: &Arc<Core>, cmd: E2eCommand) -> Reply {
    match cmd {
        E2eCommand::Status => return status(core),
        E2eCommand::Login { email, code: None } => ops_session::login_start(core, email),
        E2eCommand::Login {
            email,
            code: Some(code),
        } => {
            core.update_ui(|u| u.email = email);
            ops_session::login_verify(core, code);
        }
        E2eCommand::PairOffer => ops_pairing::offer(core, PairingMode::Code),
        E2eCommand::PairJoin { input } => ops_pairing::join(core, input),
        E2eCommand::PairConfirm { request_id, yes } => match request_id.parse::<u64>() {
            Ok(_) => ops_pairing::confirm(core, &request_id, yes),
            Err(_) => return Reply::err("request_id must be a number"),
        },
        E2eCommand::Cancel => ops_pairing::cancel(core),
        E2eCommand::ClipMode { mode } => return set_clip_mode(core, &mode),
        E2eCommand::OfferAccept { item_id } => {
            return match offers::accept_pending(core, item_id) {
                Ok(id) => Reply::ok_with(json!({"item_id": id})),
                Err(e) => Reply::err(e),
            };
        }
        E2eCommand::HistoryCopy { item_id } => {
            return match history_ops::copy_blocking(core, &item_id) {
                Ok(_) => Reply::ok(),
                Err(e) => Reply::err(e),
            };
        }
        E2eCommand::PickDevice { name } => return pick_device(core, &name),
        // A big download outlasts an IPC call: it runs on an op thread, the test polls the inbox.
        E2eCommand::HistoryDownload { item_id } => {
            if core.files.large_item(&item_id).is_none() {
                return Reply::err("no held large clip with that id");
            }
            let guard = match core.inbox.begin_download(&item_id) {
                Ok(g) => g,
                Err(_) => return Reply::err("already downloading"),
            };
            ops::spawn(core, "e2e-download", move |c| {
                if let Err(e) = inbox_runtime::download_blocking(c, &guard, &item_id, false) {
                    tracing::warn!("e2e history-download failed: {e}");
                }
            });
        }
    }
    Reply::ok()
}

/// Core `clip_mode` through the same save the Settings screen uses.
fn set_clip_mode(core: &Arc<Core>, mode: &str) -> Reply {
    let manual = match mode {
        "auto" => false,
        "manual" => true,
        _ => return Reply::err("mode must be auto or manual"),
    };
    match settings_ops::load(core).and_then(|mut f| {
        f.manual = manual;
        settings_ops::save(core, &f)
    }) {
        Ok(_) => Reply::ok(),
        Err(e) => Reply::err(format!("settings: {e}")),
    }
}

/// The DevicePicker answer, by device name (the real picker clicks a row).
fn pick_device(core: &Arc<Core>, name: &str) -> Reply {
    let node = lock(&core.devices)
        .iter()
        .find(|d| !d.is_self && d.name == name)
        .map(|d| d.node_id.clone());
    match node {
        None => {
            // Nothing stays staged after a refused answer.
            send_files::cancel_pending(core);
            Reply::err("no such device")
        }
        Some(n) => match send_files::pick(core, &n, None) {
            Ok(()) => Reply::ok(),
            Err(code) => Reply::err(code),
        },
    }
}
