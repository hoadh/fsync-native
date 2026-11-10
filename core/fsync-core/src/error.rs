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
    /// Transport failure. The text is for people (logs, UI) and not stable: never match on it.
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
    /// The peer closed the connection with `CLOSE_UNTRUSTED`: it no longer trusts us (it removed
    /// or reset this device). Distinct from `Net` (offline / unreachable).
    #[error("peer no longer trusts this device")]
    PeerRefused,
    #[error("session expired: log in again")]
    AuthExpired,
    #[error("invalid input: {0}")]
    Invalid(&'static str),
    #[error("shut down")]
    Closed,
}

pub type Result<T, E = CoreError> = std::result::Result<T, E>;

/// Longest source text [`CoreError::net_chain`] keeps per source (chars).
pub const NET_SOURCE_MAX: usize = 64;

impl CoreError {
    pub fn net(e: impl std::fmt::Display) -> Self {
        Self::Net(e.to_string())
    }

    /// Like [`Self::net`], plus the error's source chain. A QUIC stream error only says
    /// "connection lost"; why (closed by peer with a code, timed out, reset) is in its source.
    /// Each source is cut to [`NET_SOURCE_MAX`] chars: a peer chooses its close reason (up to
    /// about 1 KB), and this text reaches events and the UI.
    pub fn net_chain(e: impl std::error::Error) -> Self {
        let mut msg = e.to_string();
        let mut src = e.source();
        while let Some(s) = src {
            msg.push_str(": ");
            let text = s.to_string();
            match text.char_indices().nth(NET_SOURCE_MAX) {
                Some((cut, _)) => {
                    msg.push_str(&text[..cut]);
                    msg.push_str("...");
                }
                None => msg.push_str(&text),
            }
            src = s.source();
        }
        Self::Net(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    #[error("connection lost")]
    struct Outer(#[source] Inner);
    #[derive(Debug, thiserror::Error)]
    #[error("closed by peer: {0}")]
    struct Inner(String);

    #[test]
    fn net_chain_shows_the_source_and_cuts_a_long_peer_reason() {
        let short = CoreError::net_chain(Outer(Inner("0".into())));
        assert_eq!(
            short.to_string(),
            "network: connection lost: closed by peer: 0"
        );
        let long = CoreError::net_chain(Outer(Inner("é".repeat(1000))));
        let CoreError::Net(m) = long else { panic!() };
        let tail = m.strip_prefix("connection lost: ").unwrap();
        assert_eq!(tail.chars().count(), NET_SOURCE_MAX + 3);
        assert!(tail.ends_with("..."));
    }
}
