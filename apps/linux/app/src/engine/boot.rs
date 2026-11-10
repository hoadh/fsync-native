//! Engine bring-up on its own thread: secrets (marker-driven, never creating keys on a data dir
//! that had any), `FsyncService::new`, session restore, going online. Start order matters: the
//! app lock and IPC are already settled by `startup::prepare`.

use super::boot_secrets::obtain_secrets;
use super::delegate_adapter::DelegateAdapter;
use super::secrets_sync::{OP_SAVE_WAIT, persist, to_core_tokens};
use super::{clipboard_runtime, ops_session, pending_revoke, user_messages};
use crate::core_state::ui_state::EngineState;
use crate::core_state::{Core, lock};
use fsync_core::service::{FsyncError, FsyncService, ServiceConfig};
use fsync_linux_platform::secrets::Secrets;
use std::sync::Arc;
use zeroize::Zeroizing;

/// Platform this build announces. Linux is emitted only behind the plan's "Platform emit gate".
#[cfg(feature = "emit-platform-linux")]
const PLATFORM: fsync_proto::Platform = fsync_proto::Platform::Linux;
#[cfg(not(feature = "emit-platform-linux"))]
const PLATFORM: fsync_proto::Platform = fsync_proto::Platform::Cli;

/// Starts the boot thread unless one is already running (single flight: "Try again" may call
/// this at any time).
pub fn spawn(core: Arc<Core>) {
    if core.boot.try_begin() {
        spawn_begun(core);
    }
}

/// Runs the boot on its own thread. The caller already won `boot.try_begin()` (and may have done
/// work under that exclusion first); the slot is released when the thread ends.
pub fn spawn_begun(core: Arc<Core>) {
    /// Ends the boot even if the thread panics.
    struct Finish(Arc<Core>);
    impl Drop for Finish {
        fn drop(&mut self) {
            self.0.boot.finish();
        }
    }
    let for_error = Arc::clone(&core);
    let done = Finish(core);
    let spawned = std::thread::Builder::new()
        .name("fsync-boot".into())
        .spawn(move || run(&done.0));
    if let Err(e) = spawned {
        // The closure (and its `Finish`) was dropped with the failed spawn: the slot is free.
        tracing::error!("cannot start boot thread: {e}");
        fail(
            &for_error,
            format!("Could not start the engine thread ({e})."),
        );
    }
}

pub(super) fn fail(core: &Core, message: String) {
    core.update_ui(|u| {
        u.state = EngineState::Error;
        u.busy = false;
        u.message = message;
    });
}

fn service_config(core: &Core, s: &Secrets) -> ServiceConfig {
    ServiceConfig {
        data_dir: core.data_dir.join("core"),
        api_base: core.cfg.api.clone(),
        relay_urls: core.cfg.relays.clone(),
        device_name: core.cfg.device_name.clone(),
        platform: PLATFORM,
        device_secret: Zeroizing::new(s.device_secret),
        history_key: Zeroizing::new(s.history_key),
        force_relay: false,
        allow_insecure_http: core.cfg.allow_insecure_http,
        mdns: true,
    }
}

/// Runs once per boot attempt. Shutdown may begin at any point: the gate is checked after every
/// step that can take long, and `start()` is never reached once it is closed.
fn run(core: &Arc<Core>) {
    let Some(secrets) = obtain_secrets(core) else {
        return;
    };
    if !core.gate.is_open() {
        return;
    }
    let cfg = service_config(core, &secrets);
    let session = secrets.session.clone();
    core.update_ui(|u| {
        u.warnings.waiting_keyring = false;
        // Unlocked: the remaining bring-up is "starting", not "waiting for the keyring".
        if u.state == EngineState::WaitingForKeyring {
            u.state = EngineState::Starting;
        }
        u.email = secrets.email.clone().unwrap_or_default();
    });
    *lock(&core.secrets_mem) = Some(secrets);
    let svc = match FsyncService::new(cfg, Arc::new(DelegateAdapter::new(Arc::clone(core)))) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            let mismatch = matches!(e, FsyncError::HistoryKeyMismatch);
            let text = format!("Could not start: {}", user_messages::for_error(&e));
            core.update_ui(|u| u.history_mismatch = mismatch);
            return fail(core, text);
        }
    };
    if !core.gate.is_open() {
        return; // never started: dropping the service releases it
    }
    core.set_service(Some(Arc::clone(&svc)));
    core.update_ui(|u| u.node_id = svc.node_id());
    // The clipboard engine reads nothing until the account is ready (see ClipServiceAdapter).
    clipboard_runtime::start(core);
    let Some(tokens) = session else {
        core.update_ui(|u| u.state = EngineState::SignedOut);
        super::ops::spawn(core, "pending-revoke", |c| pending_revoke::resume(c));
        return;
    };
    match svc.set_session(to_core_tokens(&tokens)) {
        Ok(()) => ops_session::go_online_blocking(core, &svc),
        Err(FsyncError::Auth) => {
            persist(core, OP_SAVE_WAIT, |s| s.session = None);
            core.update_ui(|u| u.state = EngineState::SignedOut);
        }
        Err(e) => fail(core, user_messages::for_error(&e)),
    }
}
