//! The pairing window on the new device.
//!
//! Open for a bounded time. Several unauthenticated sessions may be in flight, but only the
//! first to pass the key-confirmation MAC *claims* the window (the TTL then stops applying and
//! everyone else is refused). `MAX_FAILED` MAC failures close it; each source gets one attempt.

use super::{DeviceInfo, PairSecret, ticket::PairTicket};
use crate::error::{CoreError, PairError, Result};
use fsync_proto::NodeId;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

/// MAC failures (wrong code guesses) tolerated before the window closes.
pub const MAX_FAILED: u8 = 3;
/// Sessions that may run the handshake concurrently.
const MAX_INFLIGHT: usize = 3;

pub(super) struct Window {
    pub(super) id: u64,
    secret: PairSecret,
    info: DeviceInfo,
    pub(super) deadline: Instant,
    done: Option<oneshot::Sender<Result<u64>>>,
    pub(super) claimed: bool,
    inflight: usize,
    failed: u8,
    seen: HashSet<NodeId>,
}

impl Window {
    fn finish(mut self, res: Result<u64>) {
        if let Some(d) = self.done.take() {
            let _ = d.send(res);
        }
    }
}

#[derive(Default)]
pub(super) struct State {
    next_id: u64,
    pub(super) window: Option<Window>,
}

#[derive(Default)]
pub struct PairingWindows(Mutex<State>);

impl PairingWindows {
    pub(super) fn lock(&self) -> Result<MutexGuard<'_, State>> {
        self.0.lock().map_err(|_| CoreError::Invalid("window lock"))
    }

    /// Open a window (errors if one is live). The receiver yields the pinned seq or the real
    /// failure of the claiming session / the guess limit.
    pub fn open(
        &self,
        secret: PairSecret,
        info: DeviceInfo,
        ttl: Duration,
    ) -> Result<oneshot::Receiver<Result<u64>>> {
        self.open_with_id(secret, info, ttl).map(|(_, rx)| rx)
    }

    /// Like `open`, also returning the window id (for `close_id`).
    pub(super) fn open_with_id(
        &self,
        secret: PairSecret,
        info: DeviceInfo,
        ttl: Duration,
    ) -> Result<(u64, oneshot::Receiver<Result<u64>>)> {
        let mut g = self.lock()?;
        if busy(&g) {
            return Err(CoreError::Invalid("a pairing window is already open"));
        }
        let (done, rx) = oneshot::channel();
        g.next_id += 1;
        let id = g.next_id;
        g.window = Some(Window {
            id,
            secret,
            info,
            deadline: Instant::now() + ttl,
            done: Some(done),
            claimed: false,
            inflight: 0,
            failed: 0,
            seen: HashSet::new(),
        });
        Ok((id, rx))
    }

    /// Close window `id` only (a newer window is left alone).
    pub(super) fn close_id(&self, id: u64) {
        if let Ok(mut g) = self.0.lock()
            && g.window.as_ref().is_some_and(|w| w.id == id)
        {
            g.window = None;
        }
    }

    /// Admit a session from `source`; refused if no live unclaimed window, too many sessions
    /// or this source already had its attempt.
    pub fn admit(self: &Arc<Self>, source: NodeId) -> Result<PairTicket> {
        let mut g = self.lock()?;
        match g.window.as_mut() {
            Some(w)
                if !w.claimed
                    && w.deadline > Instant::now()
                    && w.inflight < MAX_INFLIGHT
                    && !w.seen.contains(&source) =>
            {
                w.inflight += 1;
                Ok(PairTicket::new(
                    Arc::clone(self),
                    w.id,
                    source,
                    w.secret.clone(),
                    w.info.clone(),
                ))
            }
            _ => Err(PairError::NoWindow.into()),
        }
    }

    /// A window is live or already claimed: `open` would be refused.
    pub fn is_busy(&self) -> bool {
        self.0.lock().map(|g| busy(&g)).unwrap_or(true)
    }

    /// An unclaimed live window exists (the accept gate admits unpaired peers only then).
    pub fn is_open(&self) -> bool {
        self.0
            .lock()
            .map(|g| {
                g.window
                    .as_ref()
                    .is_some_and(|w| !w.claimed && w.deadline > Instant::now())
            })
            .unwrap_or(false)
    }

    /// TTL elapsed: close the window unless a session already claimed it. `true` if closed.
    pub fn expire(&self) -> bool {
        let Ok(mut g) = self.0.lock() else {
            return true;
        };
        if g.window.as_ref().is_some_and(|w| w.claimed) {
            return false;
        }
        g.window = None;
        true
    }

    pub fn close(&self) {
        if let Ok(mut g) = self.0.lock() {
            g.window = None;
        }
    }

    pub(super) fn end(&self, id: u64, source: NodeId, claimed: bool, res: Result<u64>) {
        let Ok(mut g) = self.0.lock() else { return };
        let Some(w) = g.window.as_mut().filter(|w| w.id == id) else {
            return;
        };
        w.inflight = w.inflight.saturating_sub(1);
        if claimed {
            if let Some(w) = g.window.take() {
                w.finish(res);
            }
            return;
        }
        w.seen.insert(source);
        if matches!(res, Err(CoreError::Pair(PairError::BadMac))) {
            w.failed += 1;
            if w.failed >= MAX_FAILED
                && let Some(w) = g.window.take()
            {
                w.finish(Err(PairError::BadMac.into()));
            }
        }
    }
}

fn busy(g: &State) -> bool {
    g.window
        .as_ref()
        .is_some_and(|w| w.claimed || w.deadline > Instant::now())
}
