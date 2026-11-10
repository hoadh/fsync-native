//! Reading the X11 CLIPBOARD owner, including INCR (needed to read big copies). Every read uses
//! its own short-lived connection and window, so it never competes with the watcher's event
//! queue and a slow owner only costs the caller's timeout. The conversion carries the selection
//! timestamp of the change that was classified: an owner that has since taken a newer selection
//! refuses it (ICCCM), so content that was not vetted is never read.

use super::x11_conn::{X11, connect};
use super::{ChangeReader, ReadError};
use std::time::{Duration, Instant};
use x11rb::NONE;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt as _, Property, Window};

pub struct X11Reader {
    /// Selection timestamp from the XFixes event (0 = current time).
    pub time: u32,
    /// X display of the watcher (`None` = `$DISPLAY`).
    pub display: Option<String>,
}

fn io(e: impl std::fmt::Display) -> ReadError {
    ReadError::Io(e.to_string())
}

/// Converts the selection to `target` and returns the raw property bytes.
fn fetch(
    x: &X11,
    win: Window,
    target: Atom,
    time: u32,
    cap: usize,
    timeout: Duration,
) -> Result<(Atom, Vec<u8>), ReadError> {
    let deadline = Instant::now() + timeout;
    let left = || {
        deadline
            .checked_duration_since(Instant::now())
            .ok_or(ReadError::Timeout)
    };
    let prop = x.atoms.FSYNC_SELECTION;
    x.conn.delete_property(win, prop).map_err(io)?;
    x.conn
        .convert_selection(win, x.atoms.CLIPBOARD, target, prop, time)
        .map_err(io)?;
    x.conn.flush().map_err(io)?;
    loop {
        match x.next_event(left()?).ok_or(ReadError::Timeout)? {
            Event::SelectionNotify(e) if e.requestor == win && e.selection == x.atoms.CLIPBOARD => {
                if e.property == NONE {
                    return Err(ReadError::Io("the owner refused the conversion".into()));
                }
                break;
            }
            _ => {}
        }
    }
    let words = u32::try_from(cap / 4 + 2).unwrap_or(u32::MAX);
    let first = x
        .conn
        .get_property(true, win, prop, AtomEnum::ANY, 0, words)
        .map_err(io)?
        .reply()
        .map_err(io)?;
    if first.type_ != x.atoms.INCR {
        if first.bytes_after > 0 || first.value.len() > cap {
            return Err(ReadError::TooLarge);
        }
        return Ok((first.type_, first.value));
    }
    // INCR: the owner now sends chunks as property changes; an empty chunk ends the transfer.
    let mut out = Vec::new();
    let mut kind = x11rb::NONE;
    loop {
        match x.next_event(left()?).ok_or(ReadError::Timeout)? {
            Event::PropertyNotify(e)
                if e.window == win && e.atom == prop && e.state == Property::NEW_VALUE =>
            {
                let chunk = x
                    .conn
                    .get_property(true, win, prop, AtomEnum::ANY, 0, words)
                    .map_err(io)?
                    .reply()
                    .map_err(io)?;
                if chunk.value.is_empty() {
                    return Ok((kind, out));
                }
                if out.len() + chunk.value.len() > cap {
                    return Err(ReadError::TooLarge);
                }
                kind = chunk.type_;
                out.extend_from_slice(&chunk.value);
            }
            _ => {}
        }
    }
}

impl ChangeReader for X11Reader {
    fn read(&self, mime: &str, cap: usize, timeout: Duration) -> Result<Vec<u8>, ReadError> {
        let x = connect(self.display.as_deref()).map_err(ReadError::Io)?;
        let win = x.hidden_window().map_err(ReadError::Io)?;
        let target = x.atom(mime).map_err(ReadError::Io)?;
        let r = fetch(&x, win, target, self.time, cap, timeout).and_then(|(kind, data)| {
            // An owner that answers with no type at all gave us nothing usable.
            if kind == x11rb::NONE {
                Err(ReadError::Io("the owner returned an untyped value".into()))
            } else {
                Ok(data)
            }
        });
        let _ = x.conn.destroy_window(win);
        let _ = x.conn.flush();
        r
    }
}

/// TARGETS of the current owner as names (the MIME list of a change).
pub fn read_targets(
    display: Option<&str>,
    time: u32,
    timeout: Duration,
) -> Result<Vec<String>, ReadError> {
    let x = connect(display).map_err(ReadError::Io)?;
    let win = x.hidden_window().map_err(ReadError::Io)?;
    let (kind, raw) = fetch(&x, win, x.atoms.TARGETS, time, 64 * 1024, timeout)?;
    if kind != x.atoms.ATOM {
        return Err(ReadError::Io("TARGETS is not an atom list".into()));
    }
    let names = raw
        .as_chunks::<4>()
        .0
        .iter()
        .filter_map(|c| x.atom_name(u32::from_ne_bytes(*c)))
        .collect();
    let _ = x.conn.destroy_window(win);
    let _ = x.conn.flush();
    Ok(names)
}
