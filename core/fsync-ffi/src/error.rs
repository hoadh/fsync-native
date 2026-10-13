use fsync_core::service as svc;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FsyncError {
    #[error("not paired with that device")]
    NotPaired,
    #[error("peer is offline")]
    PeerOffline,
    #[error("network: {msg}")]
    Network { msg: String },
    #[error("data failed integrity verification")]
    Integrity,
    #[error("content exceeds the size limit")]
    TooLarge,
    #[error("authentication required")]
    Auth,
    #[error("io: {msg}")]
    Io { msg: String },
    #[error("cancelled")]
    Cancelled,
    #[error("history key does not match the stored history")]
    HistoryKeyMismatch,
    #[error("no such item")]
    NotFound,
    #[error("invalid: {msg}")]
    Invalid { msg: String },
    #[error("internal: {msg}")]
    Internal { msg: String },
}

impl From<svc::FsyncError> for FsyncError {
    fn from(e: svc::FsyncError) -> Self {
        use svc::FsyncError as E;
        match e {
            E::NotPaired => Self::NotPaired,
            E::PeerOffline => Self::PeerOffline,
            E::Network(msg) => Self::Network { msg },
            E::Integrity => Self::Integrity,
            E::TooLarge => Self::TooLarge,
            E::Auth => Self::Auth,
            E::Io(msg) => Self::Io { msg },
            E::Cancelled => Self::Cancelled,
            E::HistoryKeyMismatch => Self::HistoryKeyMismatch,
            E::NotFound => Self::NotFound,
            E::Invalid(msg) => Self::Invalid { msg },
            E::Internal(msg) => Self::Internal { msg },
        }
    }
}

impl From<FsyncError> for svc::FsyncError {
    fn from(e: FsyncError) -> Self {
        use svc::FsyncError as E;
        match e {
            FsyncError::NotPaired => E::NotPaired,
            FsyncError::PeerOffline => E::PeerOffline,
            FsyncError::Network { msg } => E::Network(msg),
            FsyncError::Integrity => E::Integrity,
            FsyncError::TooLarge => E::TooLarge,
            FsyncError::Auth => E::Auth,
            FsyncError::Io { msg } => E::Io(msg),
            FsyncError::Cancelled => E::Cancelled,
            FsyncError::HistoryKeyMismatch => E::HistoryKeyMismatch,
            FsyncError::NotFound => E::NotFound,
            FsyncError::Invalid { msg } => E::Invalid(msg),
            FsyncError::Internal { msg } => E::Internal(msg),
        }
    }
}
