//! Wayland clipboard backend over ext-data-control-v1 / zwlr-data-control-v1 (raw
//! wayland-client, see ADR 014 spike A). Compositors without either (GNOME) are `Manual`.

use super::wayland_loop::run;
use super::wayland_loop::{Cmd, Shared};
use super::wayland_proto::{Manager, Protocol, protocol_in};
use super::wayland_watch::WlState;
use super::{Backend, BackendKind, ClipChange, OwnedEntry, WriteError};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use wayland_client::Connection;
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::wl_seat::WlSeat;

const WRITE_REPLY_WAIT: Duration = Duration::from_secs(2);
const JOIN_WAIT: Duration = Duration::from_secs(1);

/// Which data-control protocol the running compositor offers; `Err` says why there is none.
pub fn probe() -> Result<Protocol, String> {
    let conn =
        Connection::connect_to_env().map_err(|e| format!("cannot connect to Wayland: {e}"))?;
    let (globals, _queue) =
        registry_queue_init::<WlState>(&conn).map_err(|e| format!("Wayland registry: {e}"))?;
    protocol_in(&globals).ok_or_else(|| "the compositor has no data-control protocol".to_owned())
}

pub struct WaylandBackend {
    changes: Mutex<Option<Receiver<ClipChange>>>,
    cmds: Sender<Cmd>,
    wake: UnixStream,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
    done: Arc<(Mutex<bool>, std::sync::Condvar)>,
}

impl WaylandBackend {
    pub fn start() -> Result<Arc<Self>, String> {
        let conn =
            Connection::connect_to_env().map_err(|e| format!("cannot connect to Wayland: {e}"))?;
        Self::start_on(conn)
    }

    /// Starts on an existing connection (tests connect to a socket path without touching the
    /// process environment).
    pub fn start_on(conn: Connection) -> Result<Arc<Self>, String> {
        Self::start_with(conn, None)
    }

    /// `force`: use this protocol instead of preferring ext (tests the zwlr fallback).
    pub fn start_with(conn: Connection, force: Option<Protocol>) -> Result<Arc<Self>, String> {
        let (globals, mut queue) =
            registry_queue_init::<WlState>(&conn).map_err(|e| format!("Wayland registry: {e}"))?;
        let qh = queue.handle();
        let protocol = match force {
            Some(p) => p,
            None => protocol_in(&globals).ok_or("the compositor has no data-control protocol")?,
        };
        let manager = Manager::bind(&globals, &qh, protocol)?;
        let seat: WlSeat = globals
            .bind(&qh, 1..=1, ())
            .map_err(|e| format!("no seat: {e}"))?;
        let device = manager.get_device(&seat, &qh);
        let (changes_tx, changes_rx) = mpsc::channel();
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (wake_rx, wake_tx) = UnixStream::pair().map_err(|e| e.to_string())?;
        wake_rx.set_nonblocking(true).map_err(|e| e.to_string())?;
        wake_tx.set_nonblocking(true).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared::default());
        let mut state = WlState::new(
            manager,
            device,
            qh,
            conn,
            changes_tx,
            Arc::clone(&stop),
            Arc::clone(&shared),
        );
        // The selection that exists at start is not a change: receive it, announce nothing.
        queue
            .roundtrip(&mut state)
            .map_err(|e| format!("Wayland roundtrip: {e}"))?;
        state.announce = true;
        let done = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let done2 = Arc::clone(&done);
        let thread = std::thread::Builder::new()
            .name("fsync-wl".into())
            .spawn(move || {
                run(queue, state, cmd_rx, wake_rx);
                *done2.0.lock().unwrap_or_else(|p| p.into_inner()) = true;
                done2.1.notify_all();
            })
            .map_err(|e| format!("cannot start the Wayland thread: {e}"))?;
        tracing::info!("clipboard: Wayland backend started ({protocol:?})");
        Ok(Arc::new(Self {
            changes: Mutex::new(Some(changes_rx)),
            cmds: cmd_tx,
            wake: wake_tx,
            shared,
            stop,
            thread: Mutex::new(Some(thread)),
            done,
        }))
    }

    fn poke(&self) {
        use std::io::Write;
        let _ = (&self.wake).write(&[1]); // a full socket already has a wake pending
    }
}

impl Backend for WaylandBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Wayland
    }

    fn take_changes(&self) -> Option<Receiver<ClipChange>> {
        self.changes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }

    fn write(&self, entry: OwnedEntry) -> Result<(), WriteError> {
        if self.stop.load(Ordering::SeqCst) || self.shared.dead.load(Ordering::SeqCst) {
            return Err(WriteError::Closed);
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.cmds
            .send(Cmd::Write(Arc::new(entry), tx))
            .map_err(|_| WriteError::Closed)?;
        self.poke();
        rx.recv_timeout(WRITE_REPLY_WAIT)
            .map_err(|_| WriteError::Failed("the clipboard thread did not answer".into()))?
    }

    fn owned(&self) -> bool {
        self.shared.owned.load(Ordering::SeqCst)
    }

    fn shutdown(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.poke();
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
            tracing::warn!("clipboard: Wayland thread did not stop within {JOIN_WAIT:?}");
        }
    }
}
