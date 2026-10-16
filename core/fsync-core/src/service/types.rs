//! Plain data crossing the service boundary (the FFI mirrors these as records/enums).

use fsync_proto::{ClipKind, Platform};

#[derive(Clone, PartialEq, Eq)]
pub struct SessionTokens {
    pub access: String,
    pub refresh: String,
    pub account_id: String,
}

// Tokens are secrets.
impl std::fmt::Debug for SessionTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionTokens(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceStatus {
    Offline,
    Lan,
    Relay,
}

#[derive(Clone, Debug)]
pub struct Device {
    /// 64 hex chars.
    pub node_id: String,
    pub name: String,
    pub platform: Platform,
    /// Transport of the live connection, if any.
    pub status: DeviceStatus,
    /// The backend says the device is connected (a connection may not exist yet).
    pub online: bool,
    pub last_seen_ms: u64,
    pub is_self: bool,
}

#[derive(Clone, Debug)]
pub struct RepInfo {
    pub mime: String,
    pub size: u64,
}

#[derive(Clone, Debug)]
pub struct FileInfo {
    pub file_id: u32,
    pub name: String,
    pub rel_path: String,
    pub size: u64,
}

/// A validated offer from a paired device (clipboard content or a file push).
#[derive(Clone, Debug)]
pub struct ClipItem {
    pub item_id: String,
    pub from: String,
    pub from_name: String,
    pub kind: ClipKind,
    pub ts_ms: u64,
    pub preview: Option<String>,
    pub reps: Vec<RepInfo>,
    pub files: Vec<FileInfo>,
    pub total_size: u64,
    /// "Send to..." push (files only).
    pub push: bool,
}

pub struct ClipRep {
    pub mime: String,
    pub data: Vec<u8>,
}

pub struct LocalClip {
    pub reps: Vec<ClipRep>,
    /// Absolute paths; directories are expanded.
    pub files: Vec<String>,
    pub preview: Option<String>,
    /// Concealed content (password managers): sent, never stored in history here.
    pub sensitive: bool,
}

#[derive(Clone, Debug)]
pub struct Published {
    pub item_id: String,
    /// Nothing was sent: the content equals what we just applied from a peer.
    pub echo: bool,
    /// Peers that accepted the offer.
    pub delivered: Vec<String>,
    /// Offline, unreachable, or dropped the offer (rate limit, invalid).
    pub failed: Vec<String>,
    /// Peers that no longer trust this device (it was removed or reset there).
    pub refused: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairingMode {
    Code,
    Qr,
}

#[derive(Clone, Debug)]
pub struct PairingOffer {
    pub qr_payload: Option<String>,
    pub code: Option<String>,
    pub expires_ms: u64,
}

#[derive(Clone, Debug)]
pub struct PairingPeer {
    /// Pass to `pairing_confirm`; a newer request invalidates older ids.
    pub request_id: u64,
    pub node_id: String,
    pub name: String,
    pub platform: Platform,
    /// Three words to compare with the other screen.
    pub sas_words: String,
}

#[derive(Clone, Debug)]
pub enum PairingEvent {
    ConfirmRequest(PairingPeer),
    Completed { peer: String, seq: u64 },
    Failed { reason: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferDirection {
    Send,
    Receive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferState {
    Started,
    Progress,
    Done,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct TransferEvent {
    pub transfer_id: u64,
    pub item_id: String,
    pub file_id: Option<u32>,
    pub direction: TransferDirection,
    pub state: TransferState,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// Where a received file was placed (`Done`, Receive).
    pub path: Option<String>,
    /// Failure reason.
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountState {
    /// First device: genesis list created and pinned.
    Created,
    /// Pinned and in sync.
    Ready,
    /// The account has devices and this one is not among them: pair it.
    NeedsPairing,
}

#[derive(Clone, Debug)]
pub enum AccountEvent {
    ListAdvanced {
        seq: u64,
    },
    SelfRemoved,
    /// A device list with a new root (same account) is waiting for `confirm_trust_reset` /
    /// `decline_trust_reset`. Announced once per offered list; again with a new `reset_id`
    /// if that list grows before it is answered.
    TrustReset {
        reset_id: String,
        devices: u32,
        /// Names of the devices in the offered list, as chosen by whoever made the reset.
        device_names: Vec<String>,
    },
    HistoryReset {
        backup: String,
    },
    SessionExpired,
}

/// A pending trust reset offered by another device of the account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustResetInfo {
    /// Lowercase hex of the offered list's head hash; pass to confirm / decline.
    pub reset_id: String,
    /// Devices the new list contains.
    pub devices: u32,
    /// Their names, as chosen by whoever made the reset: show them as untrusted text.
    pub device_names: Vec<String>,
}
