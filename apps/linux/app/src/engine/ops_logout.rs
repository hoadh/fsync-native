//! Sign-out with a pending revoke.

use super::inbox_runtime;
use super::ops::{service_or_notify, spawn};
use super::pending_revoke;
use super::secrets_sync::{OP_SAVE_WAIT, persist, read};
use crate::core_state::Core;
use crate::core_state::gate::OpKind;
use crate::core_state::ui_state::EngineState;
use std::sync::Arc;

/// Signs out. The pending revoke is saved BEFORE the server call so a quit in between still
/// leaves a retry for the next launch; it is dropped only once the server settled it.
pub fn logout(core: &Arc<Core>) {
    let Some(svc) = service_or_notify(core) else {
        return;
    };
    core.gate.invalidate(OpKind::Account);
    core.update_ui(|u| {
        u.busy = true;
        u.message.clear();
    });
    // The account's offers, held clips and history rows go with it; transfers stop.
    inbox_runtime::reset_account_files(core);
    spawn(core, "logout", move |c| {
        let api = c.cfg.api.clone();
        let refresh = read(c, |s| s.session.as_ref().map(|t| t.refresh.clone())).flatten();
        let pending =
            refresh.map(|r| pending_revoke::Pending::new(&api, &r, pending_revoke::now_secs()));
        if let Some(p) = &pending {
            persist(c, OP_SAVE_WAIT, |s| {
                s.pending_revoke =
                    pending_revoke::with_entry(s.pending_revoke.as_deref(), p.clone())
            });
        }
        let settled = match svc.logout_confirmed() {
            Ok(settled) => settled,
            Err(e) => {
                tracing::warn!("logout: {e}");
                false
            }
        };
        persist(c, OP_SAVE_WAIT, |s| {
            s.session = None;
            s.email = None;
            // Only the entry this sign-out saved is dropped once settled; entries of earlier
            // or concurrent sign-outs stay.
            if let (true, Some(p)) = (settled, &pending) {
                s.pending_revoke =
                    pending_revoke::with_attempt_applied(s.pending_revoke.as_deref(), p, None);
            }
        });
        c.set_devices(Vec::new());
        c.update_pairing(|p| *p = Default::default());
        c.update_ui(|u| {
            u.busy = false;
            u.email.clear();
            u.trust_reset = None;
            u.state = EngineState::SignedOut;
        });
    });
}
