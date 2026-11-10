//! Core pairing events (callback thread).

use super::ops::spawn;
use super::ops_pairing::show_peer;
use super::ops_session;
use crate::core_state::ui_state::{PairingPhase, PairingUi};
use crate::core_state::{Core, lock};
use fsync_core::service::{PairingEvent, PairingPeer};
use std::sync::Arc;

/// Core pairing callback (runs on the callback thread: nothing here may block).
pub fn on_event(core: &Arc<Core>, ev: PairingEvent) {
    match ev {
        PairingEvent::ConfirmRequest(peer) => on_confirm_request(core, peer),
        PairingEvent::Completed { peer, .. } => {
            tracing::info!("paired with {peer}");
            core.update_pairing(|p| {
                p.phase = PairingPhase::Done;
                p.stray_rejects = 0;
                p.message.clear();
            });
            // Named thread: the daemon call may block, and the callback thread must not.
            let notifier = std::thread::Builder::new()
                .name("fsync-notify".into())
                .spawn(|| {
                    fsync_linux_platform::notify::show(
                        "Pairing complete",
                        "The devices are now paired.",
                    );
                });
            if let Err(e) = notifier {
                tracing::warn!("cannot start notification thread: {e}");
            }
            // The account state and device list changed (a new device becomes `Ready`).
            if let Some(svc) = core.service() {
                let c = Arc::clone(core);
                spawn(core, "post-pairing", move |_| {
                    ops_session::go_online_blocking(&c, &svc);
                });
            }
        }
        PairingEvent::Failed { reason } => {
            if lock(&core.pairing).phase == PairingPhase::Idle {
                return; // the user cancelled; the late event is just the echo
            }
            core.update_pairing(|p| {
                if skip_stray_failed(p) {
                    return;
                }
                p.phase = PairingPhase::Failed;
                p.stray_rejects = 0;
                p.message = format!("Pairing failed: {reason}");
            });
        }
    }
}

/// A `Failed` that answers one of our stray rejections is not the pairing's failure: consume it.
fn skip_stray_failed(p: &mut PairingUi) -> bool {
    let active = matches!(p.phase, PairingPhase::Answering | PairingPhase::Joining);
    if active && p.stray_rejects > 0 {
        p.stray_rejects -= 1;
        return true;
    }
    false
}

/// Core queues events asynchronously, so a confirmation can arrive in a phase that does not
/// expect one. Offering/Confirm show it; in an active phase (Answering, Joining) only that
/// request is rejected, never the pairing in flight; when nothing is active the core is told to
/// drop its (re-inserted) state. A rejected request makes core emit `Failed` or nothing; the
/// pairing page always has Cancel, so the UI cannot get stuck waiting.
fn on_confirm_request(core: &Arc<Core>, peer: PairingPeer) {
    let mut phase = PairingPhase::Idle;
    core.update_pairing(|p| {
        phase = p.phase.clone();
        if matches!(p.phase, PairingPhase::Offering | PairingPhase::Confirm) {
            show_peer(p, peer.clone());
        } else if matches!(p.phase, PairingPhase::Answering | PairingPhase::Joining) {
            p.stray_rejects += 1; // counted before the reject is sent: its Failed echo is skipped
        }
    });
    match phase {
        PairingPhase::Offering | PairingPhase::Confirm => {}
        PairingPhase::Answering | PairingPhase::Joining => {
            tracing::info!("stray pairing confirmation rejected");
            super::ops_pairing::reject_request(core, peer.request_id);
        }
        PairingPhase::Idle | PairingPhase::Done | PairingPhase::Failed => {
            tracing::info!("late pairing confirmation ignored");
            super::ops_pairing::cancel_in_core(core);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ui(phase: PairingPhase, strays: u32) -> PairingUi {
        PairingUi {
            phase,
            stray_rejects: strays,
            ..PairingUi::default()
        }
    }

    #[test]
    fn failed_echoes_of_stray_rejections_are_skipped_one_by_one() {
        let mut p = ui(PairingPhase::Answering, 2);
        assert!(skip_stray_failed(&mut p));
        assert!(skip_stray_failed(&mut p));
        assert!(!skip_stray_failed(&mut p), "a real failure is not skipped");
        assert_eq!(p.stray_rejects, 0);
    }

    #[test]
    fn nothing_is_skipped_outside_active_phases_or_without_strays() {
        assert!(!skip_stray_failed(&mut ui(PairingPhase::Answering, 0)));
        assert!(!skip_stray_failed(&mut ui(PairingPhase::Offering, 1)));
        assert!(!skip_stray_failed(&mut ui(PairingPhase::Done, 1)));
    }

    #[test]
    fn a_reset_clears_the_counter() {
        let p = ui(PairingPhase::Joining, 3);
        assert_eq!(p.stray_rejects, 3);
        // What cancel/reset/offer do:
        assert_eq!(PairingUi::default().stray_rejects, 0);
    }
}
