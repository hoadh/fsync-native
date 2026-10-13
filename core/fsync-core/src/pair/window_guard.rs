//! An open pairing window that closes itself when dropped.

use super::{DeviceInfo, PairSecret, PairingWindows};
use crate::error::{CoreError, PairError, Result};
use std::{sync::Arc, time::Duration};
use tokio::sync::oneshot;

pub struct OpenPairing {
    windows: Arc<PairingWindows>,
    id: u64,
    ttl: Duration,
    rx: oneshot::Receiver<Result<u64>>,
}

impl PairingWindows {
    /// Opens a window now (errors if one is live). Dropping the result closes that window,
    /// whatever way the owning task ends (done, failed, aborted).
    pub fn open_guarded(
        self: &Arc<Self>,
        secret: PairSecret,
        info: DeviceInfo,
        ttl: Duration,
    ) -> Result<OpenPairing> {
        let (id, rx) = self.open_with_id(secret, info, ttl)?;
        Ok(OpenPairing {
            windows: Arc::clone(self),
            id,
            ttl,
            rx,
        })
    }
}

impl OpenPairing {
    /// Waits for a peer to pair; returns the pinned list seq.
    pub async fn wait(mut self) -> Result<u64> {
        // The TTL only bounds the wait for a peer that knows the secret. Once a session has
        // claimed the window it runs to its own (bounded) end and we report that outcome.
        let out = match tokio::time::timeout(self.ttl, &mut self.rx).await {
            Ok(r) => r,
            Err(_) if self.windows.expire() => return Err(PairError::Timeout.into()),
            Err(_) => (&mut self.rx).await,
        };
        out.unwrap_or(Err(CoreError::Closed))
    }
}

impl Drop for OpenPairing {
    fn drop(&mut self) {
        self.windows.close_id(self.id);
    }
}
