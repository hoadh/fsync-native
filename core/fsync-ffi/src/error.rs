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
    #[error("a recent sign-in is required: sign in again and retry")]
    ReauthRequired,
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
    /// Added after v2, last on purpose (uniffi error cases are positional): another engine holds
    /// `engine.lock` in this data directory. The message is the text `Invalid` used to carry.
    #[error("invalid: another fsync engine is already running on this data directory")]
    EngineAlreadyRunning,
    /// Added last (positional): the other device removed or reset this one. Unlike `NotPaired`
    /// the device is still in the local list; pair again to send to it.
    #[error("the other device removed this one")]
    PeerRefused,
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
            E::ReauthRequired => Self::ReauthRequired,
            E::Io(msg) => Self::Io { msg },
            E::Cancelled => Self::Cancelled,
            E::HistoryKeyMismatch => Self::HistoryKeyMismatch,
            E::NotFound => Self::NotFound,
            E::Invalid(msg) => Self::Invalid { msg },
            E::EngineAlreadyRunning => Self::EngineAlreadyRunning,
            E::PeerRefused => Self::PeerRefused,
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
            FsyncError::ReauthRequired => E::ReauthRequired,
            FsyncError::Io { msg } => E::Io(msg),
            FsyncError::Cancelled => E::Cancelled,
            FsyncError::HistoryKeyMismatch => E::HistoryKeyMismatch,
            FsyncError::NotFound => E::NotFound,
            FsyncError::Invalid { msg } => E::Invalid(msg),
            FsyncError::EngineAlreadyRunning => E::EngineAlreadyRunning,
            FsyncError::PeerRefused => E::PeerRefused,
            FsyncError::Internal { msg } => E::Internal(msg),
        }
    }
}
