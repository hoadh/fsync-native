//! X11 event loop (one thread, "fsync-x11"): XFixes owner changes of CLIPBOARD become
//! `ClipChange`s (TARGETS first, source process from XRes + WM_CLASS), and SelectionRequest /
//! SelectionClear drive our own entry (`x11_owner`).

use super::source_app::{for_pid, parse_wm_class};
use super::x11_conn::X11;
use super::x11_owner::Owner;
use super::x11_read::{X11Reader, read_targets};
use super::{ClipChange, SourceApp};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;
use x11rb::NONE;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::res::{ClientIdMask, ClientIdSpec, ConnectionExt as _};
use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEventMask};
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, Window};

/// Budget for the owner to list its targets; the quit message waits behind it at most this long.
const TARGETS_TIMEOUT: Duration = Duration::from_millis(800);

/// Subscribes to CLIPBOARD owner changes on `win`.
pub fn subscribe(x: &X11, win: Window) -> Result<(), String> {
    x.conn
        .xfixes_query_version(5, 0)
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| format!("XFixes is required: {e}"))?;
    let mask = SelectionEventMask::SET_SELECTION_OWNER
        | SelectionEventMask::SELECTION_WINDOW_DESTROY
        | SelectionEventMask::SELECTION_CLIENT_CLOSE;
    x.conn
        .xfixes_select_selection_input(win, x.atoms.CLIPBOARD, mask)
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| format!("XFixes subscribe: {e}"))?;
    // XRes is optional (PID of the owner); without it `_NET_WM_PID` is the fallback.
    let _ = x.conn.res_query_version(1, 2).map(|c| c.reply());
    Ok(())
}

/// Process and window class of the selection owner window, as far as X can tell.
fn source_of(x: &X11, owner: Window) -> Option<SourceApp> {
    let spec = [ClientIdSpec {
        client: owner,
        mask: ClientIdMask::LOCAL_CLIENT_PID,
    }];
    let mut pid = x
        .conn
        .res_query_client_ids(&spec)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| r.ids.first().and_then(|id| id.value.first().copied()))
        .filter(|p| *p > 0);
    if pid.is_none() {
        pid = x
            .conn
            .get_property(false, owner, x.atoms._NET_WM_PID, AtomEnum::CARDINAL, 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().and_then(|mut v| v.next()))
            .filter(|p| *p > 0);
    }
    let classes = x
        .conn
        .get_property(false, owner, x.atoms.WM_CLASS, AtomEnum::STRING, 0, 64)
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|r| parse_wm_class(&r.value))
        .unwrap_or_default();
    (pid.is_some() || !classes.is_empty()).then(|| for_pid(pid, classes))
}

fn announce(
    x: &X11,
    display: &Option<String>,
    owner: Window,
    time: u32,
    changes: &Sender<ClipChange>,
) {
    let mimes = match read_targets(display.as_deref(), time, TARGETS_TIMEOUT) {
        Ok(m) => m,
        Err(e) => {
            tracing::debug!("clipboard: cannot list targets: {e}");
            return;
        }
    };
    let own = super::echo::is_own(&mimes);
    let source = if own { None } else { source_of(x, owner) };
    // A closed receiver means the sync engine stopped: nothing left to tell.
    let _ = changes.send(ClipChange {
        mimes,
        source,
        reader: Arc::new(X11Reader {
            time,
            display: display.clone(),
        }),
    });
}

/// Runs until the quit message, a broken connection, or the receiver is gone.
pub fn run(
    x: Arc<X11>,
    display: Option<String>,
    win: Window,
    owner: Arc<Owner>,
    changes: Sender<ClipChange>,
) {
    let (jobs, queue) = std::sync::mpsc::channel::<(Window, u32)>();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (ax, adisplay, astop) = (Arc::clone(&x), display.clone(), Arc::clone(&stop));
    let announcer = std::thread::Builder::new()
        .name("fsync-x11-announce".into())
        .spawn(move || {
            while let Ok(mut job) = queue.recv() {
                // Only the newest owner change matters: older ones are stale by now.
                while let Ok(newer) = queue.try_recv() {
                    job = newer;
                }
                if astop.load(std::sync::atomic::Ordering::SeqCst) {
                    break; // shutting down: no new X connections
                }
                announce(&ax, &adisplay, job.0, job.1, &changes);
            }
        })
        .map_err(|e| tracing::error!("cannot start the X11 announcer: {e}"))
        .ok();
    loop {
        let ev = match x.conn.wait_for_event() {
            Ok(ev) => ev,
            Err(e) => {
                tracing::warn!("clipboard: X11 connection lost: {e}");
                break;
            }
        };
        match ev {
            Event::XfixesSelectionNotify(e) if e.selection == x.atoms.CLIPBOARD => {
                if e.owner == NONE || e.owner == owner.win {
                    continue; // cleared, or our own write
                }
                // Off this thread: listing targets can take up to 800 ms and must not delay
                // SelectionRequest answers for our own entry.
                let _ = jobs.send((e.owner, e.selection_timestamp));
            }
            Event::SelectionRequest(req) if req.owner == win => owner.answer(&x, &req),
            Event::SelectionClear(c) if c.owner == win => owner.lost(&x),
            Event::ClientMessage(m) if m.type_ == x.atoms.FSYNC_QUIT => break,
            _ => {}
        }
    }
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    owner.release(&x);
    drop(jobs); // ends the announcer after the change it is reading
    if let Some(t) = announcer {
        let _ = t.join();
    }
}
