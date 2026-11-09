//! Account events from core (list advanced, removed, trust reset, session expired).

use super::ops::{service_or_notify, show_error, spawn};
use super::secrets_sync::{OP_SAVE_WAIT, persist};
use super::{inbox_runtime, ops_devices, ops_session};
use crate::core_state::ui_state::EngineState;
use crate::core_state::{Core, lock};
use fsync_core::service::{AccountEvent, FsyncService};
use std::sync::Arc;

/// Shows a trust reset another device made, if one is waiting.
pub fn announce_pending_resets(core: &Core, svc: &FsyncService) {
    if let Some(first) = svc.pending_trust_resets().into_iter().next() {
        core.update_ui(|u| u.trust_reset = Some((first.reset_id, first.device_names)));
    }
}

pub fn answer_trust_reset(core: &Arc<Core>, accept: bool) {
    let Some((reset_id, _)) = lock(&core.ui).trust_reset.clone() else {
        return;
    };
    let Some(svc) = service_or_notify(core) else {
        return;
    };
    core.update_ui(|u| {
        u.trust_reset = None;
        u.busy = true;
    });
    spawn(core, "trust-reset", move |c| {
        let r = if accept {
            svc.confirm_trust_reset(&reset_id).map(drop)
        } else {
            svc.decline_trust_reset(&reset_id)
        };
        match r {
            Ok(()) => {
                c.update_ui(|u| u.busy = false);
                ops_session::go_online_blocking(c, &svc);
            }
            Err(e) => show_error(c, &e),
        }
    });
}

/// Callback-thread entry: only cheap state changes, real work goes to op threads.
pub fn on_event(core: &Arc<Core>, ev: AccountEvent) {
    match ev {
        AccountEvent::ListAdvanced { .. } => ops_devices::refresh(core),
        AccountEvent::SelfRemoved => {
            // This device left the account: its offers, held clips and rows go too.
            inbox_runtime::reset_account_files(core);
            core.update_ui(|u| u.state = EngineState::NeedsPairing);
            ops_devices::refresh(core);
        }
        AccountEvent::TrustReset {
            reset_id,
            device_names,
            ..
        } => core.update_ui(|u| u.trust_reset = Some((reset_id, device_names))),
        AccountEvent::HistoryReset { .. } => {
            core.update_ui(|u| {
                u.message = "The clipboard history was damaged and has been reset.".into()
            });
        }
        AccountEvent::SessionExpired => {
            inbox_runtime::reset_account_files(core);
            let c = Arc::clone(core);
            spawn(core, "session-expired", move |_| {
                persist(&c, OP_SAVE_WAIT, |s| s.session = None);
                c.update_ui(|u| {
                    u.state = EngineState::SignedOut;
                    u.message = "Please sign in again.".into();
                });
            });
        }
    }
}
