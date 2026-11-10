//! One session's right to run the handshake against the open pairing window.

use super::{DeviceInfo, PairSecret, window::PairingWindows};
use crate::error::{CoreError, PairError, Result};
use fsync_proto::NodeId;
use std::{sync::Arc, time::Instant};

/// One session's right to run the handshake against the open window.
pub struct PairTicket {
    windows: Arc<PairingWindows>,
    id: u64,
    source: NodeId,
    claimed: bool,
    ended: bool,
    /// Seq of the list this device pinned in the session; a dropped ticket reports it.
    pinned: Option<u64>,
    pub secret: PairSecret,
    pub info: DeviceInfo,
}

impl PairTicket {
    pub(super) fn new(
        windows: Arc<PairingWindows>,
        id: u64,
        source: NodeId,
        secret: PairSecret,
        info: DeviceInfo,
    ) -> Self {
        Self {
            windows,
            id,
            source,
            claimed: false,
            ended: false,
            pinned: None,
            secret,
            info,
        }
    }

    /// Call once the peer passed key confirmation: the window is now this session's alone and
    /// its TTL stops applying. Errors if another session won or the window is gone.
    pub fn claim(&mut self) -> Result<()> {
        let mut g = self.windows.lock()?;
        match g.window.as_mut() {
            Some(w) if w.id == self.id && !w.claimed && w.deadline > Instant::now() => {
                w.claimed = true;
                self.claimed = true;
                Ok(())
            }
            _ => Err(PairError::NoWindow.into()),
        }
    }

    /// The session pinned list `seq`: if the session is dropped from now on (shutdown during the
    /// drain), the window still reports this success.
    pub fn set_pinned(&mut self, seq: u64) {
        self.pinned = Some(seq);
    }

    /// Report the session outcome (a claimed session consumes the window).
    pub fn finish(mut self, res: Result<u64>) {
        self.ended = true;
        self.windows.end(self.id, self.source, self.claimed, res);
    }
}

impl Drop for PairTicket {
    fn drop(&mut self) {
        if !self.ended {
            let res = self.pinned.ok_or(CoreError::Closed);
            self.windows.end(self.id, self.source, self.claimed, res);
        }
    }
}
