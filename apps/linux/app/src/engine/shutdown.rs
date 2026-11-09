//! Ordered shutdown with a wall-clock deadline: close the gate, stop IPC, stop the clipboard
//! (no new reads or writes, in-flight pastes cut within about 1 s, sources dropped), wait briefly
//! for an engine bring-up and going-online in flight, flush secrets (2 s, only if a save failed),
//! stop the engine (what is left of the deadline, at most 3 s), drop the service, then ask QML
//! to quit. If a step hangs, the deadline logs which one and the process exits anyway.

use super::clipboard_runtime;
use super::secrets_sync::persist;
use crate::core_state::{Core, lock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Step budget, worst case, against the 6 s deadline (the flush comes BEFORE the engine stop so
/// it can never fall outside the deadline, and the engine stop takes what is left):
/// - With a save pending (the flush runs): stop ipc ~0 + clipboard 1.0 (real worst ~1.35: budget,
///   watcher join 0.25, floors) + boot wait 0.5 + in-flight wait 0.5 + flush 2.0 = 4.35 s,
///   then engine stop min(3.0, 6.0 - 4.35 - 0.3 margin) = 1.35 s -> 5.7 s at most before "quit".
/// - Without one (no flush): clipboard worst ~1.85 (budget 1.5) + 0.5 + 0.5 = 2.85 s, engine
///   stop 3.0 s -> 5.85 s, inside the 6 s deadline.
///
/// The flush only runs when a save failed earlier, and the clipboard normally stops in well under
/// 100 ms, so the normal path is far shorter. The clipboard stops BEFORE the engine: nothing
/// may publish or fetch through a service that is being stopped.
pub const DEADLINE: Duration = Duration::from_secs(6);
const MARGIN: Duration = Duration::from_millis(300);
const STOP_WAIT: Duration = Duration::from_secs(3);
const BOOT_WAIT: Duration = Duration::from_millis(500);
const FLUSH_WAIT: Duration = Duration::from_secs(2);

static STARTED: AtomicBool = AtomicBool::new(false);
/// Set when every step before "quit" is done (engine stopped, secrets flushed).
static STOPPED: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

fn mark_stopped() {
    *lock(&STOPPED.0) = true;
    STOPPED.1.notify_all();
}

/// Waits (at most the deadline, which also ends the process) for a running shutdown to finish.
fn wait_stopped() {
    let mut done = lock(&STOPPED.0);
    let deadline = std::time::Instant::now() + DEADLINE;
    while !*done {
        let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) else {
            return;
        };
        done = STOPPED
            .1
            .wait_timeout(done, left)
            .unwrap_or_else(|p| p.into_inner())
            .0;
    }
}

/// Starts the deadline watchdog; returns the step marker it reports.
fn arm_deadline() -> Arc<Mutex<&'static str>> {
    let step = Arc::new(Mutex::new("start"));
    let watchdog_step = Arc::clone(&step);
    let _ = std::thread::Builder::new()
        .name("fsync-quit-deadline".into())
        .spawn(move || {
            std::thread::sleep(DEADLINE);
            tracing::error!(
                "shutdown deadline hit while in step {:?}; exiting",
                *lock(&watchdog_step)
            );
            std::process::exit(0);
        });
    step
}

/// Starts the shutdown once (later calls are no-ops). Safe from any thread.
pub fn begin(core: Option<Arc<Core>>) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let step = arm_deadline();
    let worker = std::thread::Builder::new()
        .name("fsync-shutdown".into())
        .spawn(move || run_steps(core, &step, true));
    if let Err(e) = worker {
        tracing::error!("cannot start shutdown thread: {e}");
        std::process::exit(0);
    }
}

/// The Qt loop ended without a shutdown (a quit path that bypassed `begin`): run the same
/// ordered steps here, under the same deadline, so the engine is stopped, the secrets are
/// flushed and the IPC socket is removed before the process exits.
pub fn finish_after_event_loop(core: &Arc<Core>) {
    if STARTED.swap(true, Ordering::SeqCst) {
        // `begin` is already doing it: do not let the process exit mid-stop.
        wait_stopped();
        return;
    }
    tracing::warn!("event loop ended without a shutdown request; stopping the engine now");
    let step = arm_deadline();
    run_steps(Some(Arc::clone(core)), &step, false);
}

fn mark(step: &Mutex<&'static str>, name: &'static str) {
    *lock(step) = name;
}

/// `notify_qt`: the last step asks QML to quit; false when there is no event loop any more.
fn run_steps(core: Option<Arc<Core>>, step: &Mutex<&'static str>, notify_qt: bool) {
    let Some(core) = core else {
        std::process::exit(0);
    };
    let started = Instant::now();
    mark(step, "close gate");
    core.gate.close();
    // Offer and History downloads stop (and drop their partial copies) within the deadline.
    core.files.cancel_all();
    mark(step, "stop ipc");
    if let Some(server) = lock(&core.ipc).take() {
        server.stop();
    }
    mark(step, "stop clipboard");
    let unsaved = lock(&core.ui).warnings.secrets_unsaved;
    clipboard_runtime::stop(
        &core,
        if unsaved {
            clipboard_runtime::STOP_BUDGET_UNSAVED
        } else {
            clipboard_runtime::STOP_BUDGET
        },
    );
    // An engine bring-up in flight sees the closed gate before it starts anything; wait briefly
    // so `core.service()` below is final (a hung bring-up only costs this wait).
    mark(step, "wait for boot");
    core.boot.wake();
    if !core.boot.wait_idle(BOOT_WAIT) {
        tracing::warn!("engine bring-up still running after {BOOT_WAIT:?}");
    }
    if !core.starting.wait_idle(BOOT_WAIT) {
        tracing::warn!("going online still running after {BOOT_WAIT:?}");
    }
    mark(step, "flush secrets");
    if lock(&core.ui).warnings.secrets_unsaved {
        persist(&core, FLUSH_WAIT, |_| {});
    }
    mark(step, "stop engine");
    // Whatever the earlier steps used, stop gets only what is left of the deadline.
    let stop_wait = STOP_WAIT.min(
        DEADLINE
            .saturating_sub(started.elapsed())
            .saturating_sub(MARGIN),
    );
    if let Some(svc) = core.service() {
        let (tx, rx) = mpsc::channel();
        // Control call on its own thread: a hung stop only costs the 3 s wait.
        let spawned = std::thread::Builder::new()
            .name("fsync-stop".into())
            .spawn(move || {
                let r = svc.stop();
                let _ = tx.send(r);
            });
        if spawned.is_ok() && rx.recv_timeout(stop_wait).is_err() {
            tracing::warn!("engine stop did not finish within {stop_wait:?}");
        }
    }
    mark(step, "drop service");
    core.set_service(None);
    mark_stopped();
    mark(step, "quit");
    if !notify_qt {
        return; // the caller leaves the process
    }
    if core.hub.has_engine() {
        core.hub.quit();
    } else {
        std::process::exit(0);
    }
}
