//! X11 clipboard backend (thin, ADR 014): XFixes watch, INCR on receive only, owner window for
//! writes. Used for X11 sessions; XWayland is not a source for GNOME auto sync (dropped).

use super::x11_conn::{X11, connect};
use super::x11_owner::Owner;
use super::x11_watch::{run, subscribe};
use super::{Backend, BackendKind, ClipChange, OwnedEntry, WriteError};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt as _, EventMask};

const JOIN_WAIT: Duration = Duration::from_secs(1);

pub struct X11Backend {
    x: Arc<X11>,
    owner: Arc<Owner>,
    changes: Mutex<Option<Receiver<ClipChange>>>,
    stopped: AtomicBool,
    thread: Mutex<Option<JoinHandle<()>>>,
    done: Arc<(Mutex<bool>, Condvar)>,
}

/// Whether an X server answers on `DISPLAY` (used by detection).
pub fn probe() -> Result<(), String> {
    connect(None).map(|_| ())
}

impl X11Backend {
    /// `display`: `None` = `$DISPLAY`.
    pub fn start(display: Option<String>) -> Result<Arc<Self>, String> {
        let x = Arc::new(connect(display.as_deref())?);
        let win = x.hidden_window()?;
        subscribe(&x, win)?;
        let owner = Arc::new(Owner::new(win));
        let (tx, rx) = mpsc::channel();
        let done = Arc::new((Mutex::new(false), Condvar::new()));
        let (x2, o2, d2) = (Arc::clone(&x), Arc::clone(&owner), Arc::clone(&done));
        let thread = std::thread::Builder::new()
            .name("fsync-x11".into())
            .spawn(move || {
                run(x2, display, win, o2, tx);
                *d2.0.lock().unwrap_or_else(|p| p.into_inner()) = true;
                d2.1.notify_all();
            })
            .map_err(|e| format!("cannot start the X11 thread: {e}"))?;
        tracing::info!("clipboard: X11 backend started");
        Ok(Arc::new(Self {
            x,
            owner,
            changes: Mutex::new(Some(rx)),
            stopped: AtomicBool::new(false),
            thread: Mutex::new(Some(thread)),
            done,
        }))
    }

    fn send_quit(&self) {
        let ev = ClientMessageEvent::new(32, self.owner.win, self.x.atoms.FSYNC_QUIT, [0u32; 5]);
        let _ = self
            .x
            .conn
            .send_event(false, self.owner.win, EventMask::NO_EVENT, ev);
        let _ = self.x.conn.flush();
    }
}

impl Backend for X11Backend {
    fn kind(&self) -> BackendKind {
        BackendKind::X11
    }

    fn take_changes(&self) -> Option<Receiver<ClipChange>> {
        self.changes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }

    fn write(&self, entry: OwnedEntry) -> Result<(), WriteError> {
        if self.stopped.load(Ordering::SeqCst) {
            return Err(WriteError::Closed);
        }
        self.owner.set(&self.x, &entry)
    }

    fn owned(&self) -> bool {
        self.owner.owned.load(Ordering::SeqCst)
    }

    fn shutdown(&self) {
        if self.stopped.swap(true, Ordering::SeqCst) {
            return;
        }
        self.send_quit();
        let (flag, cv) = &*self.done;
        let guard = flag.lock().unwrap_or_else(|p| p.into_inner());
        let finished = cv
            .wait_timeout_while(guard, JOIN_WAIT, |d| !*d)
            .map(|(g, _)| *g)
            .unwrap_or(false);
        if finished {
            if let Some(t) = self.thread.lock().unwrap_or_else(|p| p.into_inner()).take() {
                let _ = t.join();
            }
        } else {
            tracing::warn!("clipboard: X11 thread did not stop within {JOIN_WAIT:?}");
        }
    }
}
