//! The Wayland thread's event loop: dispatch, flush, poll the connection and the wake socket.

use super::wayland_watch::WlState;
use super::{OwnedEntry, WriteError};
use rustix::event::{PollFd, PollFlags};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender};
use wayland_client::EventQueue;

pub(super) enum Cmd {
    Write(Arc<OwnedEntry>, SyncSender<Result<(), WriteError>>),
}

/// Flags the backend handle reads from other threads.
#[derive(Default)]
pub(super) struct Shared {
    pub owned: AtomicBool,
    /// The compositor removed the device or the connection broke: the backend is unusable.
    pub dead: AtomicBool,
}

fn drain(wake: &UnixStream) {
    use std::io::Read;
    let mut buf = [0u8; 64];
    while matches!((&*wake).read(&mut buf), Ok(n) if n > 0) {}
}

/// The loop. Returns when `stop` is set, the device finished or the connection broke.
pub(super) fn run(
    mut queue: EventQueue<WlState>,
    mut st: WlState,
    cmds: Receiver<Cmd>,
    wake: UnixStream,
) {
    loop {
        while let Ok(cmd) = cmds.try_recv() {
            st.command(cmd);
        }
        if st.stop.load(Ordering::SeqCst) || st.shared.dead.load(Ordering::SeqCst) {
            break;
        }
        if queue.dispatch_pending(&mut st).is_err() || queue.flush().is_err() {
            st.shared.dead.store(true, Ordering::SeqCst);
            break;
        }
        let Some(guard) = queue.prepare_read() else {
            continue; // events were queued meanwhile
        };
        let wl_fd = guard.connection_fd();
        let mut fds = [
            PollFd::new(&wl_fd, PollFlags::IN | PollFlags::ERR),
            PollFd::new(&wake, PollFlags::IN),
        ];
        match rustix::event::poll(&mut fds, None) {
            Ok(_) | Err(rustix::io::Errno::INTR) => {}
            Err(_) => {
                st.shared.dead.store(true, Ordering::SeqCst);
                break;
            }
        }
        if fds[1].revents().contains(PollFlags::IN) {
            drain(&wake);
        }
        if fds[0].revents().intersects(PollFlags::IN | PollFlags::ERR) {
            match guard.read() {
                Ok(_) => {}
                Err(wayland_client::backend::WaylandError::Io(e))
                    if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => {
                    st.shared.dead.store(true, Ordering::SeqCst);
                    break;
                }
            }
        } else {
            drop(guard);
        }
    }
    st.close();
}
