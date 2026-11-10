//! Ways out of the waiting and error pages: "Try again", "Unlock keyring", "Reset history".

use super::ops::spawn;
use super::secrets_sync::{persist, read};
use super::{boot, ops_session, user_messages};
use crate::core_state::ui_state::EngineState;
use crate::core_state::{Core, lock};
use fsync_core::service::FsyncService;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// "Try again": re-runs the engine bring-up when boot failed (no service exists), otherwise
/// retries going online. Refused while anything is busy; boot is single flight, so a double
/// click starts nothing twice.
pub fn retry(core: &Arc<Core>) {
    if lock(&core.ui).busy {
        return;
    }
    if core.service().is_some() {
        ops_session::go_online(core);
        return;
    }
    if lock(&core.ui).state != EngineState::Error || !core.boot.try_begin() {
        return;
    }
    start_boot_again(core);
}

/// Resets the UI to "starting" and hands the already-won boot slot to a new boot thread.
fn start_boot_again(core: &Arc<Core>) {
    core.update_ui(|u| {
        u.state = EngineState::Starting;
        u.message.clear();
        u.history_mismatch = false;
    });
    boot::spawn_begun(Arc::clone(core));
}

/// "Unlock keyring": allows one more unlock prompt, wakes a boot that waits for the keyring,
/// and (after boot) re-saves the in-memory secrets, which is what hits the locked keyring.
pub fn unlock_keyring(core: &Arc<Core>) {
    // One prompt thread at a time: a second click while it runs would only queue more jobs.
    if UNLOCK_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Some(handle) = core.secrets.as_ref() {
        handle.request_unlock();
    }
    core.boot.wake();
    // Long wait: the user is typing a password; nothing but this op thread waits for it. A
    // no-op while boot has not produced the secrets yet (nothing to save).
    spawn(core, "unlock-keyring", |c| {
        if read(c, |_| ()).is_some() {
            persist(c, UNLOCK_SAVE_WAIT, |_| {});
        } else if let Some(handle) = c.secrets.as_ref() {
            // Boot is still waiting for the keyring: this open is what shows the prompt.
            if let Err(e) = handle.ensure_open_within(UNLOCK_SAVE_WAIT) {
                tracing::info!("keyring still not usable after the unlock request: {e}");
            }
        }
        // The prompt ended (unlocked, dismissed or timed out): a boot still sleeping in its
        // keyring poll re-checks now instead of after the rest of the 10 s.
        c.boot.wake();
        UNLOCK_RUNNING.store(false, Ordering::SeqCst);
    });
    // `spawn` does nothing once shutdown began; do not leave the flag set then.
    if !core.gate.is_open() {
        UNLOCK_RUNNING.store(false, Ordering::SeqCst);
    }
}

static UNLOCK_RUNNING: AtomicBool = AtomicBool::new(false);

/// A bit longer than the secrets thread's own prompt bound (120 s).
const UNLOCK_SAVE_WAIT: Duration = Duration::from_secs(125);

/// "Reset history" after `HistoryKeyMismatch`: core moves the old history aside (nothing is
/// deleted) and the device must pair again (`ensure_account` then reports `NeedsPairing`, or
/// `Created` for an empty account; it cannot report `Ready` because the pinned trust is gone).
/// Only run while boot has failed with that error. The boot slot is held for the whole reset,
/// so no boot or retry can touch the directory meanwhile.
pub fn reset_history(core: &Arc<Core>) {
    let allowed = {
        let ui = lock(&core.ui);
        ui.history_mismatch && ui.state == EngineState::Error && !ui.busy
    };
    if !allowed || core.service().is_some() || !core.boot.try_begin() {
        return;
    }
    core.update_ui(|u| {
        u.busy = true;
        u.message.clear();
    });
    let c = Arc::clone(core);
    let spawned = std::thread::Builder::new()
        .name("fsync-op-reset-history".into())
        .spawn(
            move || match FsyncService::reset_history(&c.data_dir.join("core")) {
                Ok(()) => {
                    c.update_ui(|u| u.busy = false);
                    start_boot_again(&c);
                }
                Err(e) => {
                    c.boot.finish();
                    let text = user_messages::for_error(&e);
                    c.update_ui(|u| {
                        u.busy = false;
                        u.message = text;
                    });
                }
            },
        );
    if let Err(e) = spawned {
        core.boot.finish();
        core.update_ui(|u| {
            u.busy = false;
            u.message = format!("Could not start the reset ({e}).");
        });
    }
}
