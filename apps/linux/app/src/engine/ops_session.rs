//! Sign-in, going online, sign-out.

use super::ops::{service_or_notify, show_error, spawn};
use super::secrets_sync::{OP_SAVE_WAIT, from_core_tokens, persist};
use super::{ops_account, pending_revoke};
use crate::core_state::gate::OpKind;
use crate::core_state::ui_state::EngineState;
use crate::core_state::{Core, lock};
use fsync_core::service::{AccountState, FsyncError, FsyncService};
use std::sync::Arc;

pub fn login_start(core: &Arc<Core>, email: String) {
    let email = email.trim().to_owned();
    if !email.contains('@') || email.len() < 3 {
        core.update_ui(|u| u.message = "Enter a valid email address.".into());
        return;
    }
    let Some(svc) = service_or_notify(core) else {
        return;
    };
    let generation = core.gate.begin(OpKind::Login);
    core.update_ui(|u| {
        u.busy = true;
        u.message.clear();
    });
    spawn(core, "login-start", move |c| {
        let r = svc.login_start(&email);
        if !c.gate.is_current(OpKind::Login, generation) {
            return;
        }
        match r {
            Ok(()) => c.update_ui(|u| {
                u.busy = false;
                u.email = email;
                u.state = EngineState::AwaitingCode;
            }),
            Err(e) => show_error(c, &e),
        }
    });
}

pub fn login_verify(core: &Arc<Core>, code: String) {
    let code = code.trim().to_owned();
    let email = lock(&core.ui).email.clone();
    if code.is_empty() || email.is_empty() {
        core.update_ui(|u| u.message = "Enter the code from the email.".into());
        return;
    }
    let Some(svc) = service_or_notify(core) else {
        return;
    };
    let generation = core.gate.begin(OpKind::Login);
    core.update_ui(|u| {
        u.busy = true;
        u.message.clear();
    });
    spawn(core, "login-verify", move |c| {
        let r = svc.login_verify(&email, &code);
        if !c.gate.is_current(OpKind::Login, generation) {
            return;
        }
        match r {
            Ok(tokens) => {
                let saved = from_core_tokens(&tokens);
                persist(c, OP_SAVE_WAIT, |s| {
                    s.session = Some(saved);
                    s.email = Some(email);
                });
                go_online_blocking(c, &svc);
            }
            Err(e) => show_error(c, &e),
        }
    });
}

pub fn back_to_email(core: &Core) {
    core.update_ui(|u| {
        if u.state == EngineState::AwaitingCode {
            u.state = EngineState::SignedOut;
        }
        u.message.clear();
        u.busy = false;
    });
}

pub fn go_online(core: &Arc<Core>) {
    let Some(svc) = service_or_notify(core) else {
        return;
    };
    spawn(core, "go-online", move |c| {
        go_online_blocking(c, &svc);
    });
}

/// Starts the node and maps `ensure_account` to a UI state. Safe to repeat (retry). Does nothing
/// once shutdown began: an engine started after `stop` would never be stopped.
pub fn go_online_blocking(core: &Arc<Core>, svc: &Arc<FsyncService>) {
    let generation = core.gate.begin(OpKind::Account);
    core.update_ui(|u| {
        u.busy = true;
        u.node_id = svc.node_id();
    });
    // Only `start()` is counted: shutdown closes the gate, then waits for a start that already
    // passed it. `ensure_account` runs after a fresh gate check and is stopped by `stop()`.
    let started = match core.starting.enter(&core.gate) {
        Some(_counted) => svc.start(),
        None => return,
    };
    let result = match started {
        Ok(()) if core.gate.is_open() => svc.ensure_account(),
        Ok(()) => return,
        Err(e) => Err(e),
    };
    if !core.gate.is_current(OpKind::Account, generation) {
        return;
    }
    match result {
        Ok(account) => {
            let next = match account {
                AccountState::NeedsPairing => EngineState::NeedsPairing,
                AccountState::Created | AccountState::Ready => EngineState::Ready,
            };
            core.update_ui(|u| {
                u.busy = false;
                u.state = next;
                u.message.clear();
            });
            super::ops_devices::refresh_blocking(core, svc);
            ops_account::announce_pending_resets(core, svc);
            spawn(core, "pending-revoke", |c| pending_revoke::resume(c));
        }
        Err(FsyncError::EngineAlreadyRunning) => {
            core.update_ui(|u| {
                u.busy = false;
                u.state = EngineState::Error;
                u.message = super::user_messages::ALREADY_RUNNING.into();
            });
        }
        Err(FsyncError::Auth) => {
            persist(core, OP_SAVE_WAIT, |s| {
                s.session = None;
            });
            core.update_ui(|u| {
                u.busy = false;
                u.state = EngineState::SignedOut;
                u.message = "Please sign in again.".into();
            });
        }
        // Network trouble, throttling, a pin to another account: keep the session, offer a retry.
        Err(e) => {
            core.update_ui(|u| u.state = EngineState::Offline);
            show_error(core, &e);
        }
    }
}
