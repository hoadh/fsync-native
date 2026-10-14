//! App-facing error: the FFI `FsyncError` mirrors this one variant for variant.

use crate::error::{CoreError, PairError, TransferError};
use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum FsyncError {
    #[error("not paired with that device")]
    NotPaired,
    #[error("peer is offline")]
    PeerOffline,
    #[error("network: {0}")]
    Network(String),
    #[error("data failed integrity verification")]
    Integrity,
    #[error("content exceeds the size limit")]
    TooLarge,
    #[error("authentication required")]
    Auth,
    #[error("a recent sign-in is required: sign in again and retry")]
    ReauthRequired,
    #[error("io: {0}")]
    Io(String),
    #[error("cancelled")]
    Cancelled,
    #[error("history key does not match the stored history")]
    HistoryKeyMismatch,
    #[error("no such item")]
    NotFound,
    #[error("invalid: {0}")]
    Invalid(String),
    #[error("internal: {0}")]
    Internal(String),
}

pub type SvcResult<T> = Result<T, FsyncError>;

impl FsyncError {
    pub(crate) fn invalid(msg: &str) -> Self {
        Self::Invalid(msg.to_owned())
    }
}

impl From<CoreError> for FsyncError {
    fn from(e: CoreError) -> Self {
        match e {
            CoreError::Transfer(TransferError::Integrity) => Self::Integrity,
            CoreError::Transfer(TransferError::Denied) | CoreError::NotTrusted(_) => {
                Self::NotPaired
            }
            CoreError::Transfer(t) => Self::Network(t.to_string()),
            CoreError::TooLarge { .. } => Self::TooLarge,
            CoreError::Pair(PairError::Rejected) => Self::Cancelled,
            CoreError::Pair(p) => Self::Invalid(p.to_string()),
            CoreError::Api(a) if a.status == 401 && a.is("reauth_required") => Self::ReauthRequired,
            CoreError::Api(a) if a.status == 401 => Self::Auth,
            CoreError::Api(a) if a.status == 429 || a.status >= 500 => Self::Network(a.to_string()),
            CoreError::Api(a) => Self::Invalid(a.to_string()),
            CoreError::AuthExpired => Self::Auth,
            CoreError::Io(e) => Self::Io(e.to_string()),
            CoreError::Db(e) => Self::Io(e.to_string()),
            CoreError::Store(m) => Self::Io(m),
            CoreError::Net(m) | CoreError::Http(m) | CoreError::Ws(m) => Self::Network(m),
            CoreError::Invalid(m) => Self::Invalid(m.to_owned()),
            CoreError::Closed => Self::Cancelled,
            CoreError::HistoryKeyMismatch => Self::HistoryKeyMismatch,
            e @ (CoreError::Proto(_) | CoreError::Codec(_)) => Self::Invalid(e.to_string()),
        }
    }
}
