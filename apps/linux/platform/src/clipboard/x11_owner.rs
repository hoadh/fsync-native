//! Owning the X11 CLIPBOARD: our hidden window answers SelectionRequest from the watcher
//! thread's event loop. Thin by design (ADR 014): no INCR on the sending side, so an entry
//! larger than the server's maximum request is refused at write time and shown as skipped.
//! MULTIPLE and TIMESTAMP are not offered.

use super::x11_conn::X11;
use super::{OwnedEntry, WriteError};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, EventMask, PropMode, SELECTION_NOTIFY_EVENT,
    SelectionNotifyEvent, SelectionRequestEvent, Window,
};
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{CURRENT_TIME, NONE};

/// Headroom for the request header inside the server's maximum request length.
const REQUEST_HEADROOM: usize = 64;

struct Target {
    /// Bytes to serve (STRING is Latin-1 converted).
    data: Vec<u8>,
    /// Property type of the reply.
    kind: Atom,
}

pub struct Owner {
    pub win: Window,
    pub owned: AtomicBool,
    table: Mutex<HashMap<Atom, Target>>,
    /// Serialises `set` (swap table, take the selection) against `lost` (check, clear): a stale
    /// SelectionClear must not wipe an entry that was just set.
    gate: Mutex<()>,
}

/// STRING is ISO 8859-1: only text whose chars all fit is offered under it.
fn latin1(utf8: &[u8]) -> Option<Vec<u8>> {
    std::str::from_utf8(utf8)
        .ok()?
        .chars()
        .map(|c| u8::try_from(u32::from(c)).ok())
        .collect()
}

impl Owner {
    pub fn new(win: Window) -> Self {
        Self {
            win,
            owned: AtomicBool::new(false),
            table: Mutex::new(HashMap::new()),
            gate: Mutex::new(()),
        }
    }

    /// Becomes the owner and serves `entry` until another client takes the selection.
    pub fn set(&self, x: &X11, entry: &OwnedEntry) -> Result<(), WriteError> {
        let fail = |e: &dyn std::fmt::Display| WriteError::Failed(e.to_string());
        let limit = x
            .conn
            .maximum_request_bytes()
            .saturating_sub(REQUEST_HEADROOM);
        let mut table = HashMap::new();
        for rep in &entry.reps {
            if rep.data.len() > limit {
                // Never keep serving the previous entry as if it were the new one.
                self.release(x);
                return Err(WriteError::TooLargeForX11(rep.data.len()));
            }
            for name in &rep.offered {
                let (data, kind) = match name.as_str() {
                    "STRING" => match latin1(&rep.data) {
                        Some(d) => (d, x.atoms.STRING),
                        None => continue,
                    },
                    "TEXT" | "UTF8_STRING" => (rep.data.clone(), x.atoms.UTF8_STRING),
                    _ => (rep.data.clone(), x.atom(name).map_err(|e| fail(&e))?),
                };
                let atom = match name.as_str() {
                    "STRING" => x.atoms.STRING,
                    "TEXT" => x.atoms.TEXT,
                    "UTF8_STRING" => x.atoms.UTF8_STRING,
                    _ => kind,
                };
                table.entry(atom).or_insert(Target { data, kind });
            }
        }
        let _serial = self.gate.lock().unwrap_or_else(|p| p.into_inner());
        *self.table.lock().unwrap_or_else(|p| p.into_inner()) = table;
        x.conn
            .set_selection_owner(self.win, x.atoms.CLIPBOARD, CURRENT_TIME)
            .map_err(|e| fail(&e))?;
        x.conn.flush().map_err(|e| fail(&e))?;
        let now = x
            .conn
            .get_selection_owner(x.atoms.CLIPBOARD)
            .map_err(|e| fail(&e))?
            .reply()
            .map_err(|e| fail(&e))?;
        if now.owner != self.win {
            return Err(WriteError::Failed(
                "the X server did not grant the selection".into(),
            ));
        }
        self.owned.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// SelectionClear: someone else owns the selection now. A clear that was queued before a
    /// newer `set` (we re-took the selection) is stale: the server is asked who owns it.
    pub fn lost(&self, x: &X11) {
        let _serial = self.gate.lock().unwrap_or_else(|p| p.into_inner());
        let still_ours = x
            .conn
            .get_selection_owner(x.atoms.CLIPBOARD)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|r| r.owner == self.win);
        if still_ours {
            return;
        }
        self.owned.store(false, Ordering::SeqCst);
        self.table.lock().unwrap_or_else(|p| p.into_inner()).clear();
    }

    /// Releases the selection on shutdown if it is still ours.
    pub fn release(&self, x: &X11) {
        if self.owned.swap(false, Ordering::SeqCst) {
            let still = x
                .conn
                .get_selection_owner(x.atoms.CLIPBOARD)
                .ok()
                .and_then(|c| c.reply().ok());
            if still.is_some_and(|r| r.owner == self.win) {
                let _ = x
                    .conn
                    .set_selection_owner(NONE, x.atoms.CLIPBOARD, CURRENT_TIME);
            }
        }
        self.table.lock().unwrap_or_else(|p| p.into_inner()).clear();
        let _ = x.conn.flush();
    }

    /// Answers one request; a refusal sends `property = None` as ICCCM requires.
    pub fn answer(&self, x: &X11, req: &SelectionRequestEvent) {
        let prop = if req.property == NONE {
            req.target
        } else {
            req.property
        };
        let served = req.selection == x.atoms.CLIPBOARD
            && req.owner == self.win
            && self.owned.load(Ordering::SeqCst);
        let ok = served && self.fill(x, req, prop);
        let reply = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: req.time,
            requestor: req.requestor,
            selection: req.selection,
            target: req.target,
            property: if ok { prop } else { NONE },
        };
        let _ = x
            .conn
            .send_event(false, req.requestor, EventMask::NO_EVENT, reply);
        let _ = x.conn.flush();
    }

    fn fill(&self, x: &X11, req: &SelectionRequestEvent, prop: Atom) -> bool {
        let table = self.table.lock().unwrap_or_else(|p| p.into_inner());
        let sent = if req.target == x.atoms.TARGETS {
            let mut list: Vec<u32> = table.keys().copied().collect();
            list.push(x.atoms.TARGETS);
            x.conn.change_property32(
                PropMode::REPLACE,
                req.requestor,
                prop,
                AtomEnum::ATOM,
                &list,
            )
        } else if let Some(t) = table.get(&req.target) {
            x.conn
                .change_property8(PropMode::REPLACE, req.requestor, prop, t.kind, &t.data)
        } else {
            return false;
        };
        sent.is_ok()
    }
}

#[cfg(test)]
#[path = "x11_owner_tests.rs"]
mod tests;
