//! Core pairing events -> service events.

use super::{
    delegate::Out,
    error::FsyncError,
    inner::{Inner, lock},
    types::{PairingEvent, PairingPeer},
};
use crate::events::PairingEvent as CoreEvent;
use std::sync::Arc;

impl Inner {
    /// Core pairing events -> service events. Called from the event pump.
    pub(super) fn on_pairing_event(self: &Arc<Self>, ev: CoreEvent) {
        match ev {
            CoreEvent::ConfirmRequest {
                peer,
                name,
                platform,
                sas,
                decision,
                ..
            } => {
                let Ok(mut slot) = lock(&self.pairing) else {
                    return;
                };
                slot.next_request += 1;
                let p = PairingPeer {
                    request_id: slot.next_request,
                    node_id: peer.to_string(),
                    name,
                    platform,
                    sas_words: sas,
                };
                // A newer request replaces an unanswered one, which counts as rejected.
                if let Some((_, old)) = slot.decision.replace((p.request_id, decision)) {
                    old.reject();
                }
                match slot.join_waiter.take() {
                    // The joining side gets the SAS as the return value of `pairing_join`.
                    Some(w) => {
                        let _ = w.send(Ok(p));
                    }
                    None => self.emit(Out::Pairing(PairingEvent::ConfirmRequest(p))),
                }
            }
            CoreEvent::Completed { peer, seq } => {
                if let Ok(mut s) = lock(&self.pairing) {
                    s.decision = None;
                }
                // Only a device that just became listed (no presence yet) needs to connect; the
                // existing device keeps its socket.
                if let Err(e) = self.ensure_presence() {
                    self.emit(Out::Error(e));
                }
                self.emit(Out::Pairing(PairingEvent::Completed {
                    peer: peer.to_string(),
                    seq,
                }));
            }
            CoreEvent::Failed { reason } => {
                if let Ok(mut s) = lock(&self.pairing) {
                    s.decision = None;
                    if let Some(w) = s.join_waiter.take() {
                        let _ = w.send(Err(FsyncError::Invalid(reason.clone())));
                    }
                }
                self.emit(Out::Pairing(PairingEvent::Failed { reason }));
            }
        }
    }
}
