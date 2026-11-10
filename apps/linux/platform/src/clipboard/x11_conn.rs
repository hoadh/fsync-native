//! X11 connection helpers shared by the watcher, the owner and the reader: atoms, a hidden
//! window, bounded event waits. Pure-Rust x11rb (no libxcb), one connection per role.

use rustix::event::{PollFd, PollFlags};
use rustix::time::Timespec;
use std::os::fd::AsFd;
use std::time::{Duration, Instant};
use x11rb::COPY_DEPTH_FROM_PARENT;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    Atom, ConnectionExt as _, CreateWindowAux, EventMask, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;

x11rb::atom_manager! {
    pub Atoms: AtomsCookie {
        CLIPBOARD,
        TARGETS,
        TIMESTAMP,
        INCR,
        ATOM,
        STRING,
        UTF8_STRING,
        TEXT,
        FSYNC_SELECTION,
        FSYNC_QUIT,
        _NET_WM_PID,
        WM_CLASS,
    }
}

pub struct X11 {
    pub conn: RustConnection,
    pub root: Window,
    pub atoms: Atoms,
}

/// Extra connection attempts after a transient failure (150, 300, 450 ms apart).
const CONNECT_RETRIES: u32 = 3;

/// Errors a server that is still starting can produce; everything else is final.
fn is_transient(e: &x11rb::errors::ConnectError) -> bool {
    use std::io::ErrorKind::{ConnectionRefused, ConnectionReset, UnexpectedEof};
    matches!(e, x11rb::errors::ConnectError::IoError(io)
        if matches!(io.kind(), ConnectionReset | ConnectionRefused | UnexpectedEof))
}

/// `display`: `None` = `$DISPLAY`.
pub fn connect(display: Option<&str>) -> Result<X11, String> {
    // A server that is still starting (Xvfb, on-demand XWayland) can reset the first connection;
    // retry a few times before giving up. Any other error fails at once.
    let mut attempt = 0;
    let (conn, screen) = loop {
        match RustConnection::connect(display) {
            Ok(c) => break c,
            Err(e) if attempt < CONNECT_RETRIES && is_transient(&e) => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(150 * u64::from(attempt)));
            }
            Err(e) => return Err(format!("cannot connect to X11: {e}")),
        }
    };
    let root = conn.setup().roots.get(screen).ok_or("no X11 screen")?.root;
    let atoms = Atoms::new(&conn)
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| format!("X11 atoms: {e}"))?;
    Ok(X11 { conn, root, atoms })
}

impl X11 {
    /// Hidden 1x1 input-only window that receives property and structure events.
    pub fn hidden_window(&self) -> Result<Window, String> {
        let id = self.conn.generate_id().map_err(|e| e.to_string())?;
        self.conn
            .create_window(
                COPY_DEPTH_FROM_PARENT,
                id,
                self.root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_ONLY,
                0,
                &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
            )
            .map_err(|e| e.to_string())?
            .check()
            .map_err(|e| format!("X11 window: {e}"))?;
        Ok(id)
    }

    pub fn atom(&self, name: &str) -> Result<Atom, String> {
        Ok(self
            .conn
            .intern_atom(false, name.as_bytes())
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| e.to_string())?
            .atom)
    }

    pub fn atom_name(&self, atom: Atom) -> Option<String> {
        let r = self.conn.get_atom_name(atom).ok()?.reply().ok()?;
        String::from_utf8(r.name).ok()
    }

    /// Next event, or `None` after `timeout` (also `None` when the connection broke).
    pub fn next_event(&self, timeout: Duration) -> Option<Event> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.conn.poll_for_event() {
                Ok(Some(ev)) => return Some(ev),
                Ok(None) => {}
                Err(_) => return None, // broken connection
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            let ts = Timespec {
                tv_sec: left.as_secs() as _,
                tv_nsec: left.subsec_nanos() as _,
            };
            let fd = self.conn.stream().as_fd();
            let mut fds = [PollFd::new(
                &fd,
                PollFlags::IN | PollFlags::ERR | PollFlags::HUP,
            )];
            match rustix::event::poll(&mut fds, Some(&ts)) {
                Ok(0) => return None,
                Ok(_) | Err(rustix::io::Errno::INTR) => {}
                Err(_) => return None,
            }
            // Readable: the next poll_for_event reads and parses it.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};
    use x11rb::errors::ConnectError;

    #[test]
    fn only_a_starting_server_is_retried() {
        for k in [
            ErrorKind::ConnectionReset,
            ErrorKind::ConnectionRefused,
            ErrorKind::UnexpectedEof,
        ] {
            assert!(
                is_transient(&ConnectError::IoError(Error::from(k))),
                "{k:?}"
            );
        }
        assert!(!is_transient(&ConnectError::IoError(Error::from(
            ErrorKind::PermissionDenied
        ))));
        assert!(!is_transient(&ConnectError::DisplayParsingError(
            x11rb::errors::DisplayParsingError::MalformedValue("x".into())
        )));
    }
}
