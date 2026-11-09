//! Pairing: offer (code or QR), join, SAS confirmation, cancel. Core pairing events land in
//! `on_event`.

use super::ops::{service_or_notify, spawn};
use super::{qr_image, user_messages};
use crate::core_state::Core;
use crate::core_state::gate::OpKind;
use crate::core_state::ui_state::{PairingPhase, PairingUi};
use fsync_core::service::{FsyncError, PairingMode, PairingPeer};
use std::sync::Arc;

fn failed(core: &Core, e: &FsyncError) {
    let text = user_messages::for_error(e);
    core.update_pairing(|p| {
        p.phase = PairingPhase::Failed;
        p.message = text;
    });
}

pub(super) fn show_peer(p: &mut PairingUi, peer: PairingPeer) {
    p.phase = PairingPhase::Confirm;
    p.sas = peer.sas_words;
    p.peer_name = peer.name;
    p.request_id = peer.request_id;
    p.message.clear();
}

/// Starts `next` atomically unless a pairing is in progress (then nothing changes and false is
/// returned), so a double click cannot start a second offer. A finished one (done/failed) may
/// be replaced directly.
fn begin_from_idle(core: &Core, next: PairingPhase) -> bool {
    let mut started = false;
    core.update_pairing(|p| {
        if matches!(
            p.phase,
            PairingPhase::Idle | PairingPhase::Done | PairingPhase::Failed
        ) {
            *p = PairingUi {
                phase: next,
                ..PairingUi::default()
            };
            started = true;
        }
    });
    started
}

pub fn offer(core: &Arc<Core>, mode: PairingMode) {
    let Some(svc) = service_or_notify(core) else {
        return;
    };
    if !begin_from_idle(core, PairingPhase::Offering) {
        return;
    }
    let generation = core.gate.begin(OpKind::Pairing);
    spawn(core, "pair-offer", move |c| {
        let r = svc.pairing_offer(mode);
        if !c.gate.is_current(OpKind::Pairing, generation) {
            return;
        }
        match r {
            Ok(offer) => {
                let qr_url = offer
                    .qr_payload
                    .as_deref()
                    .map(qr_image::png_data_url)
                    .transpose();
                match qr_url {
                    Ok(url) => c.update_pairing(|p| {
                        p.code = offer.code.unwrap_or_default();
                        p.qr_url = url.unwrap_or_default();
                        p.qr_text = offer.qr_payload.unwrap_or_default();
                    }),
                    Err(e) => failed(c, &FsyncError::Internal(e)),
                }
            }
            Err(e) => failed(c, &e),
        }
    });
}

pub fn join(core: &Arc<Core>, input: String) {
    let input = input.trim().to_owned();
    if input.is_empty() {
        core.update_pairing(|p| p.message = "Enter the pairing code or paste the QR text.".into());
        return;
    }
    let Some(svc) = service_or_notify(core) else {
        return;
    };
    if !begin_from_idle(core, PairingPhase::Joining) {
        return;
    }
    let generation = core.gate.begin(OpKind::Pairing);
    spawn(core, "pair-join", move |c| {
        let r = svc.pairing_join(&input);
        if !c.gate.is_current(OpKind::Pairing, generation) {
            return;
        }
        match r {
            Ok(peer) => c.update_pairing(|p| show_peer(p, peer)),
            Err(e) => failed(c, &e),
        }
    });
}

/// Answers the SAS question. `request_id` is the id the dialog rendered: an answer for anything
/// but the confirmation currently shown is refused (stale click, double click, other phase).
pub fn confirm(core: &Arc<Core>, request_id: &str, accept: bool) {
    let Some(svc) = service_or_notify(core) else {
        return;
    };
    let Ok(id) = request_id.parse::<u64>() else {
        tracing::warn!("pairing confirm with an invalid request id");
        return;
    };
    let mut accepted_click = false;
    core.update_pairing(|p| {
        if p.phase == PairingPhase::Confirm && p.request_id == id {
            accepted_click = true;
            if accept {
                p.phase = PairingPhase::Answering;
                p.message.clear();
            } else {
                *p = PairingUi::default();
            }
        }
    });
    if !accepted_click {
        tracing::info!("pairing confirm ignored: not the confirmation on screen");
        return;
    }
    let generation = core.gate.current(OpKind::Pairing);
    spawn(core, "pair-confirm", move |c| {
        if let Err(e) = svc.pairing_confirm(id, accept) {
            // A cancel (or a newer pairing) in between already moved on: do not overwrite it.
            if c.gate.is_current(OpKind::Pairing, generation) {
                failed(c, &e);
            }
        }
    });
}

/// Control call: a fresh thread, never behind a long op, allowed while shutting down.
pub fn cancel(core: &Arc<Core>) {
    core.gate.invalidate(OpKind::Pairing);
    core.update_pairing(|p| *p = PairingUi::default());
    cancel_in_core(core);
}

/// Tells core to drop its pairing state (no UI change).
pub(super) fn cancel_in_core(core: &Core) {
    let Some(svc) = core.service() else { return };
    let spawned = std::thread::Builder::new()
        .name("fsync-pair-cancel".into())
        .spawn(move || {
            if let Err(e) = svc.pairing_cancel() {
                tracing::warn!("pairing cancel: {e}");
            }
        });
    if let Err(e) = spawned {
        tracing::error!("cannot start cancel thread: {e}");
    }
}

/// Rejects one request in core without touching the pairing in flight.
pub(super) fn reject_request(core: &Core, request_id: u64) {
    let Some(svc) = core.service() else { return };
    let spawned = std::thread::Builder::new()
        .name("fsync-pair-reject".into())
        .spawn(move || {
            if let Err(e) = svc.pairing_confirm(request_id, false) {
                tracing::debug!("pairing reject: {e}");
            }
        });
    if let Err(e) = spawned {
        tracing::error!("cannot start reject thread: {e}");
    }
}

pub fn reset(core: &Core) {
    core.update_pairing(|p| *p = PairingUi::default());
}
