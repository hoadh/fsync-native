//! Crate-wide error type.

use fsync_proto::{ProtoError, pairing::AbortReason};
use thiserror::Error;

/// Non-2xx backend response (`{code, message}`).
#[derive(Debug, Clone, Error)]
#[error("api error {status} {code}: {message}")]
pub struct ApiError {
    pub status: u16,
    pub code: String,
    pub message: String,
}

impl ApiError {
    pub fn is(&self, code: &str) -> bool {
        self.code == code
    }
}

/// Failure inside the pair/1 handshake.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PairError {
    #[error("confirmation MAC mismatch (wrong code or active attacker)")]
    BadMac,
    #[error("pairing rejected by the user")]
    Rejected,
    #[error("peer aborted pairing: {0:?}")]
    PeerAborted(AbortReason),
    #[error("pairing timed out")]
    Timeout,
    #[error("no pairing window is open")]
    NoWindow,
    #[error("no pinned device list: only a paired device can add others")]
    NoPinnedList,
    #[error("pairing protocol violation: {0}")]
    Protocol(&'static str),
}

/// Failure of a file/1 transfer, matchable by callers.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum TransferError {
    #[error("received data failed blake3 verification")]
    Integrity,
    #[error("source changed since it was offered")]
    Changed,
    #[error("peer denied the request")]
    Denied,
    #[error("item is no longer available on the sender")]
    NotFound,
    #[error("transfer timed out")]
    Timeout,
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error(transparent)]
    Transfer(#[from] TransferError),
    #[error("clipboard item of {size} bytes exceeds the {max} byte limit")]
    TooLarge { size: u64, max: u64 },
    #[error(transparent)]
    Proto(#[from] ProtoError),
    #[error(transparent)]
    Pair(#[from] PairError),
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("codec: {0}")]
    Codec(#[from] postcard::Error),
    #[error("network: {0}")]
    Net(String),
    #[error("http transport: {0}")]
    Http(String),
    /// The backend could not be reached (connect refused/reset/unreachable, DNS, timeout). Kept
    /// apart from `Http` (TLS, bad or oversized body), which may mean a forged backend.
    #[error("http transport: {0}")]
    Unreachable(String),
    #[error("websocket: {0}")]
    Ws(String),
    #[error("history database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("history key does not match the stored history")]
    HistoryKeyMismatch,
    #[error("trust store: {0}")]
    Store(String),
    #[error("not trusted: {0}")]
    NotTrusted(String),
    #[error("session expired: log in again")]
    AuthExpired,
    #[error("invalid input: {0}")]
    Invalid(&'static str),
    #[error("shut down")]
    Closed,
}

pub type Result<T, E = CoreError> = std::result::Result<T, E>;

impl CoreError {
    pub fn net(e: impl std::fmt::Display) -> Self {
        Self::Net(e.to_string())
    }
}
