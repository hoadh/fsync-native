//! pair/1: add a new device to the account.
//!
//! Existing device (E) dials the new device (N). Code mode runs SPAKE2 over the 6-digit code,
//! QR mode derives the key from the 128-bit QR secret. Both modes then exchange key-confirmation
//! MACs, derive 3 SAS words that the user compares on both screens, and E signs + publishes the
//! extended device list only after the user accepts. The backend sees the code, so the SAS
//! check is what stops a malicious backend from relaying a man in the middle.

mod initiator;
mod initiator_steps;
mod kdf;
mod responder;
mod spake;
mod ticket;
mod window;
mod window_guard;
mod wire;

pub use initiator::run_initiator;
pub use kdf::{PairKeys, Role, qr_key, transcript};
pub use responder::run_responder;
pub use ticket::PairTicket;
pub use window::{MAX_FAILED, PairingWindows};
pub use window_guard::OpenPairing;

use crate::error::Result;
use fsync_proto::{NodeId, Platform, SignedDeviceList, pairing::PairMode};
use std::time::Duration;
use zeroize::Zeroizing;

/// Secret shared out-of-band (6-digit code or QR secret). Zeroized on drop.
#[derive(Clone)]
pub enum PairSecret {
    Code(Zeroizing<String>),
    Qr(Zeroizing<[u8; 16]>),
}

impl PairSecret {
    pub fn mode(&self) -> PairMode {
        match self {
            Self::Code(_) => PairMode::Code,
            Self::Qr(_) => PairMode::Qr,
        }
    }

    /// Shared key material available without a network round (QR mode only).
    pub(crate) fn qr_key(&self, e: &NodeId, n: &NodeId) -> Option<Zeroizing<[u8; 32]>> {
        match self {
            Self::Qr(s) => Some(qr_key(s, e, n)),
            Self::Code(_) => None,
        }
    }
}

/// What the new device announces about itself.
#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub name: String,
    pub platform: Platform,
    /// Local only (never sent): the account this device is logged into. When set, the list
    /// received from the existing device must belong to it.
    pub account_id: Option<uuid::Uuid>,
}

/// Backend access the existing device needs while pairing (implemented over `ApiClient`).
pub trait ListBackend: Send + Sync {
    /// PUT the list with `If-Match: <prev_seq>`; a 409 must surface as `CoreError::Api`.
    fn publish(
        &self,
        list: &SignedDeviceList,
        prev_seq: u64,
    ) -> impl Future<Output = Result<()>> + Send;
    /// Fetch newer lists and ingest them into the trust manager (after a 409).
    fn sync(&self) -> impl Future<Output = Result<()>> + Send;
    /// The full chain from genesis to the current head.
    fn full_chain(&self) -> impl Future<Output = Result<Vec<SignedDeviceList>>> + Send;
}

pub(crate) const STEP_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const USER_TIMEOUT: Duration = Duration::from_secs(120);

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
