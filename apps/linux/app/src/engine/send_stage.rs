//! Staging for the DevicePicker: validate on an op thread, keep the verdict only if it is still
//! the latest request, wake the picker. `--send` (IPC) waits for the verdict with a bound; a
//! cold-start `--send` waits for the engine. The IPC thread never touches the file system.

use super::ops::spawn;
use super::send_validate::*;
use crate::core_state::{Core, lock};
use fsync_linux_platform::clipboard::path_denylist::PathDenylist;
use fsync_linux_platform::paths::process_env;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// How long `--send` waits for the verdict before answering "queued" (the result then shows in
/// the window).
const IPC_WAIT: Duration = Duration::from_secs(2);
/// A cold-start `--send` waits this long for the engine to become ready.
const COLD_START_WAIT: Duration = Duration::from_secs(120);
/// Checks running at once; each can block on a dead mount.
pub const MAX_INFLIGHT: usize = 4;

pub fn deny(core: &Core) -> PathDenylist {
    PathDenylist::from_env(&process_env, &core.data_dir)
}

fn file_size(p: &Path) -> Option<u64> {
    std::fs::metadata(p)
        .ok()
        .filter(|m| m.is_file())
        .map(|m| m.len())
}

/// Validates `raw` for stage request `request` and stages it. A refusal empties the staged
/// set (nothing stays staged after a refused request); a request that a newer one replaced
/// changes nothing (`SUPERSEDED`).
pub fn stage_with(
    core: &Core,
    deny: &PathDenylist,
    request: u64,
    raw: &[PathBuf],
) -> Result<usize, &'static str> {
    let checked = validate(deny, raw);
    // The summary reads file sizes: outside the lock.
    let staged = checked.map(|paths| {
        let summary = summarize(&paths, file_size);
        (paths, summary)
    });
    let mut pending = lock(&core.pending_send);
    if !pending.is_latest(request) {
        return Err(SUPERSEDED);
    }
    let verdict = match staged {
        Ok((paths, summary)) => {
            let n = paths.len();
            pending.set(paths, summary);
            Ok(n)
        }
        Err(code) => {
            pending.clear();
            Err(code)
        }
    };
    drop(pending);
    core.hub.poke_send();
    verdict
}

/// Forgets the staged set (refusal, cancel); the picker closes through the poke.
pub fn cancel_pending(core: &Core) {
    if lock(&core.pending_send).clear() {
        core.hub.poke_send();
    }
}

/// Sign-out: the staged set goes and checks still running are void.
pub fn reset_for_sign_out(core: &Core) {
    let mut pending = lock(&core.pending_send);
    pending.void_requests();
    let had = pending.clear();
    drop(pending);
    if had {
        core.hub.poke_send();
    }
}

pub fn refuse(core: &Core, code: &str) {
    core.update_ui(|u| u.message = refusal_text(code).into());
}

/// Counts one running check; released when dropped (also when the op thread never starts).
struct Inflight(Arc<Core>);

impl Inflight {
    fn take(core: &Arc<Core>) -> Option<Self> {
        let n = core.stage_inflight.fetch_add(1, Ordering::SeqCst);
        if n >= MAX_INFLIGHT {
            core.stage_inflight.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Self(Arc::clone(core)))
    }
}

impl Drop for Inflight {
    fn drop(&mut self) {
        self.0.stage_inflight.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Stage + open the picker on an op thread. `reply` gets the verdict (a refusal also shows in
/// the window). Too many checks at once: `busy`.
pub fn stage_and_show(
    core: &Arc<Core>,
    raw: Vec<PathBuf>,
    token: Option<String>,
    reply: Option<mpsc::Sender<Result<(), &'static str>>>,
) -> Result<(), &'static str> {
    let Some(slot) = Inflight::take(core) else {
        return Err(BUSY);
    };
    let request = lock(&core.pending_send).begin_request();
    spawn(core, "stage-send", move |c| {
        let _slot = slot;
        let verdict = stage_with(c, &deny(c), request, &raw).map(|_| ());
        match verdict {
            Ok(()) => c.hub.show_section(token, "send"),
            Err(SUPERSEDED) => {}
            Err(code) => refuse(c, code),
        }
        if let Some(tx) = reply {
            // The waiter may have given up (timeout): nothing left to tell.
            let _ = tx.send(verdict);
        }
    });
    Ok(())
}

/// IPC `--send`: the IPC thread waits at most `IPC_WAIT`; past that the launcher gets Ok
/// ("queued") and a refusal shows in the window instead. If the work never runs (shutdown) the
/// answer is `shutting-down`, not ok.
pub fn stage_bounded(
    core: &Arc<Core>,
    raw: Vec<PathBuf>,
    token: Option<String>,
) -> Result<(), &'static str> {
    let (tx, rx) = mpsc::channel();
    stage_and_show(core, raw, token, Some(tx))?;
    match rx.recv_timeout(IPC_WAIT) {
        Ok(Err(SUPERSEDED)) => Ok(()),
        Ok(verdict) => verdict,
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(()),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(SHUTTING_DOWN),
    }
}

/// `fsync-linux --send` that started this instance: staged once the engine is ready (see
/// `cold_start` for the states that refuse).
pub fn on_cold_start(core: &Arc<Core>, raw: Vec<PathBuf>) {
    spawn(core, "cold-send", move |c| {
        let deadline = Instant::now() + COLD_START_WAIT;
        loop {
            if !c.gate.is_open() {
                return;
            }
            let state = lock(&c.ui).state;
            match cold_start(state, Instant::now() >= deadline) {
                ColdStart::Stage => {
                    if let Err(code) = stage_and_show(c, raw, None, None) {
                        refuse(c, code);
                    }
                    return;
                }
                ColdStart::Refuse(code) => return refuse(c, code),
                ColdStart::Wait => std::thread::sleep(Duration::from_millis(250)),
            }
        }
    });
}
