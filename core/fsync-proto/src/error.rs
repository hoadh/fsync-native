//! Typed errors for every fallible operation in this crate.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtoError {
    #[error("postcard encode/decode failed: {0}")]
    Codec(#[from] postcard::Error),
    #[error("trailing bytes after payload")]
    TrailingBytes,
    #[error("unsupported version {0}")]
    UnsupportedVersion(u8),
    #[error("invalid ed25519 public key")]
    InvalidKey,
    #[error("signature verification failed")]
    BadSignature,
    #[error("signing key does not match list signer")]
    SignerMismatch,
    #[error("invalid genesis list: {0}")]
    InvalidGenesis(&'static str),
    #[error("device list is empty")]
    EmptyList,
    #[error("duplicate device in list")]
    DuplicateDevice,
    #[error("device not in list")]
    DeviceNotFound,
    #[error("rollback: got seq {got}, pinned seq {pinned}")]
    Rollback { got: u64, pinned: u64 },
    #[error("gap: expected seq {expected}, got {got}; fetch intermediate lists")]
    Gap { expected: u64, got: u64 },
    #[error("account id mismatch")]
    WrongAccount,
    #[error("signer is not a trusted device in the pinned list")]
    SignerNotTrusted,
    #[error("prev_hash does not match pinned list hash")]
    PrevHashMismatch,
    #[error("trust reset: same account, new genesis root; requires user confirmation")]
    TrustReset,
    #[error("frame too large: {len} > {max}")]
    FrameTooLarge { len: usize, max: usize },
    #[error("frame truncated: need {needed} more bytes")]
    Truncated { needed: usize },
    #[error("invalid node id")]
    InvalidNodeId,
    #[error("invalid qr payload: {0}")]
    InvalidQr(&'static str),
    #[error("invalid clip offer: {0}")]
    InvalidOffer(&'static str),
    #[error("invalid relative path")]
    InvalidPath,
    #[error("fetch offset not aligned to chunk group")]
    MisalignedOffset,
    #[error("device list sequence overflow")]
    SeqOverflow,
    #[error("invalid device list: {0}")]
    InvalidDeviceList(&'static str),
    #[error("invalid base64")]
    InvalidBase64,
}
