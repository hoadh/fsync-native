//! Clipboard engine life cycle: detect the backend, start the sync engine once the core service
//! exists, show its status, stop it first at shutdown. Everything here is Qt-free; the Qt side
//! (`bridge::clipboard_object`) only reads `snapshot` and calls the actions.

use super::clip_service_adapter::ClipServiceAdapter;
use super::inbox_runtime;
use crate::core_state::{Core, lock};
use fsync_linux_platform::clipboard::BackendKind;
use fsync_linux_platform::clipboard::detect::{Choice, Detection, detect, start as start_backend};
use fsync_linux_platform::clipboard::path_denylist::PathDenylist;
use fsync_linux_platform::clipboard::source_app::capability;
use fsync_linux_platform::clipboard::source_app_ipc::lookup_failing;
use fsync_linux_platform::paths::process_env;
use fsync_linux_platform::sync::{FilesEnv, SyncEngine, SyncEnv, SyncStatus};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};
use std::time::Duration;

/// Budget for stopping the clipboard (see `shutdown` for the arithmetic).
pub const STOP_BUDGET: Duration = Duration::from_millis(1500);
/// Shorter when `secrets_unsaved` is set: the flush needs its 2 s and the engine its share.
pub const STOP_BUDGET_UNSAVED: Duration = Duration::from_millis(1000);
const NOTICE_MARKER: &str = "clipboard-notice-seen";

const MANUAL_CAPABILITY: &str = "Nothing is read in the background: use the Send and Paste commands. Password hints are honoured.";

fn initial_status(det: &Detection, kind: Option<BackendKind>) -> SyncStatus {
    SyncStatus {
        backend: match kind {
            Some(k) => k.as_str().to_owned(),
            None => "Manual (Send/Paste window)".to_owned(),
        },
        reason: det.reason.clone(),
        capability: match kind {
            Some(k) => capability(k, &process_env).to_owned(),
            None => MANUAL_CAPABILITY.to_owned(),
        },
        auto: kind.is_some(),
        ..SyncStatus::default()
    }
}

/// Starts the clipboard engine once (after the core service exists). A backend that cannot
/// start degrades to Manual with the reason shown, never to a failed boot.
pub fn start(core: &Arc<Core>) {
    if lock(&core.clip).is_some() {
        return; // a boot retry: the engine keeps running across it
    }
    let mut det = detect(&process_env);
    let backend = match start_backend(&det.choice) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("clipboard backend unavailable: {e}");
            det = Detection {
                choice: Choice::Manual,
                reason: e,
            };
            None
        }
    };
    let kind = backend.as_ref().map(|b| b.kind());
    let weak: Weak<Core> = Arc::downgrade(core);
    let large_core = Weak::clone(&weak);
    let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        if let Some(c) = weak.upgrade() {
            c.hub.poke_clipboard();
            super::tray::refresh(&c);
        }
    });
    let env = SyncEnv {
        paths: PathDenylist::from_env(&process_env, &core.data_dir),
        files: Some(FilesEnv {
            inbox: inbox_runtime::inbox(core),
            on_large: Arc::new(move |item| {
                if let Some(c) = large_core.upgrade() {
                    inbox_runtime::on_large(&c, item);
                }
            }),
        }),
    };
    let service = Arc::new(ClipServiceAdapter {
        core: Arc::clone(core),
    });
    let engine = match SyncEngine::start(service, backend, env, initial_status(&det, kind), notify)
    {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("cannot start the clipboard engine: {e}");
            return;
        }
    };
    let mut slot = lock(&core.clip);
    // Shutdown closes the gate before it takes the engine: either we see the closed gate here
    // or shutdown finds the engine in the slot.
    if !core.gate.is_open() || slot.is_some() {
        drop(slot);
        engine.shutdown(STOP_BUDGET);
        return;
    }
    let wayland = kind == Some(BackendKind::Wayland);
    core.clip_notice.store(
        wayland && !core.data_dir.join(NOTICE_MARKER).exists(),
        Ordering::SeqCst,
    );
    *slot = Some(engine);
    drop(slot);
    core.hub.poke_clipboard();
    inbox_runtime::start_schedule(core);
}

/// Shutdown step: takes the engine out and stops it (bounded by `STOP_BUDGET`).
/// `budget` is `STOP_BUDGET`, or `STOP_BUDGET_UNSAVED` when secrets still need their flush.
pub fn stop(core: &Core, budget: Duration) {
    let engine = lock(&core.clip).take();
    if let Some(e) = engine {
        e.shutdown(budget);
    }
}

pub fn engine(core: &Core) -> Option<Arc<SyncEngine>> {
    lock(&core.clip).clone()
}

/// What the Qt object shows.
#[derive(Clone, Debug, Default)]
pub struct ClipUi {
    pub backend: String,
    pub reason: String,
    pub capability: String,
    pub auto: bool,
    pub paused: bool,
    pub pending: String,
    pub last_event: String,
    pub notice: bool,
}

pub fn snapshot(core: &Core) -> ClipUi {
    let Some(engine) = engine(core) else {
        return ClipUi::default();
    };
    let s = engine.status();
    ClipUi {
        pending: s
            .pending
            .map(|p| format!("{} from {}", p.kind, p.from_name))
            .unwrap_or_default(),
        backend: s.backend,
        reason: s.reason,
        capability: if s.auto && lookup_failing() {
            "Password hints only: the focused-app lookup is unavailable.".to_owned()
        } else {
            s.capability
        },
        auto: s.auto,
        paused: s.paused,
        last_event: s.last_event,
        notice: core.clip_notice.load(Ordering::SeqCst),
    }
}

/// "Do not show again": a marker file in the data dir.
pub fn dismiss_notice(core: &Core) {
    core.clip_notice.store(false, Ordering::SeqCst);
    if let Err(e) = std::fs::write(core.data_dir.join(NOTICE_MARKER), b"1") {
        tracing::warn!("cannot save the clipboard notice marker: {e}");
    }
    core.hub.poke_clipboard();
}

pub fn toggle_pause(core: &Core) {
    if let Some(e) = engine(core) {
        e.set_paused(!e.paused());
    }
}
