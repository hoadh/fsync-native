//! Shared plumbing for engine operations: each long op runs on its own short-lived thread (the
//! service is `Sync`, so nothing is queued behind a slow call); results reach the UI through
//! `Core::update_*`, which pokes the Qt objects.

use super::user_messages;
use crate::core_state::Core;
use fsync_core::service::{FsyncError, FsyncService};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

/// Runs `f` on a named thread unless shutdown began. A panic is contained and shown as a message.
pub fn spawn(core: &Arc<Core>, name: &str, f: impl FnOnce(&Arc<Core>) + Send + 'static) {
    if !core.gate.is_open() {
        return;
    }
    let c = Arc::clone(core);
    let spawned = std::thread::Builder::new()
        .name(format!("fsync-op-{name}"))
        .spawn(move || {
            if catch_unwind(AssertUnwindSafe(|| f(&c))).is_err() {
                tracing::error!("operation thread panicked");
                c.update_ui(|u| {
                    u.busy = false;
                    u.message = "Something went wrong.".into();
                });
            }
        });
    if let Err(e) = spawned {
        tracing::error!("cannot start operation thread: {e}");
        core.update_ui(|u| u.message = "Something went wrong.".into());
    }
}

/// The running service, or a visible "not ready" message.
pub fn service_or_notify(core: &Core) -> Option<Arc<FsyncService>> {
    let svc = core.service();
    if svc.is_none() {
        core.update_ui(|u| u.message = "The engine is not ready yet.".into());
    }
    svc
}

pub fn show_error(core: &Core, e: &FsyncError) {
    let text = user_messages::for_error(e);
    core.update_ui(|u| {
        u.busy = false;
        u.message = text;
    });
}
