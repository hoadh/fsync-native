//! Records and enums: pairing, transfers, account state.

use crate::types_device::Platform;
use fsync_core::service as svc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PairingMode {
    Code,
    Qr,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PairingOffer {
    pub qr_payload: Option<String>,
    pub code: Option<String>,
    pub expires_ms: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PairingPeer {
    pub request_id: u64,
    pub node_id: String,
    pub name: String,
    pub platform: Platform,
    pub sas_words: String,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum PairingEvent {
    ConfirmRequest { peer: PairingPeer },
    Completed { peer: String, seq: u64 },
    Failed { reason: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TransferDirection {
    Send,
    Receive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TransferState {
    Started,
    Progress,
    Done,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct TransferEvent {
    pub transfer_id: u64,
    pub item_id: String,
    pub file_id: Option<u32>,
    pub direction: TransferDirection,
    pub state: TransferState,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub path: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AccountState {
    Created,
    Ready,
    NeedsPairing,
}

#[derive(Clone, Debug, uniffi::Enum)]
pub enum AccountEvent {
    ListAdvanced { seq: u64 },
    SelfRemoved,
    TrustReset { devices: u32 },
    HistoryReset { backup: String },
    SessionExpired,
}

impl From<PairingMode> for svc::PairingMode {
    fn from(m: PairingMode) -> Self {
        match m {
            PairingMode::Code => Self::Code,
            PairingMode::Qr => Self::Qr,
        }
    }
}

impl From<svc::PairingOffer> for PairingOffer {
    fn from(o: svc::PairingOffer) -> Self {
        Self {
            qr_payload: o.qr_payload,
            code: o.code,
            expires_ms: o.expires_ms,
        }
    }
}

impl From<svc::PairingPeer> for PairingPeer {
    fn from(p: svc::PairingPeer) -> Self {
        Self {
            request_id: p.request_id,
            node_id: p.node_id,
            name: p.name,
            platform: p.platform.into(),
            sas_words: p.sas_words,
        }
    }
}

impl From<svc::PairingEvent> for PairingEvent {
    fn from(e: svc::PairingEvent) -> Self {
        match e {
            svc::PairingEvent::ConfirmRequest(p) => Self::ConfirmRequest { peer: p.into() },
            svc::PairingEvent::Completed { peer, seq } => Self::Completed { peer, seq },
            svc::PairingEvent::Failed { reason } => Self::Failed { reason },
        }
    }
}

impl From<svc::TransferEvent> for TransferEvent {
    fn from(e: svc::TransferEvent) -> Self {
        Self {
            transfer_id: e.transfer_id,
            item_id: e.item_id,
            file_id: e.file_id,
            direction: match e.direction {
                svc::TransferDirection::Send => TransferDirection::Send,
                svc::TransferDirection::Receive => TransferDirection::Receive,
            },
            state: match e.state {
                svc::TransferState::Started => TransferState::Started,
                svc::TransferState::Progress => TransferState::Progress,
                svc::TransferState::Done => TransferState::Done,
                svc::TransferState::Failed => TransferState::Failed,
                svc::TransferState::Cancelled => TransferState::Cancelled,
            },
            bytes_done: e.bytes_done,
            bytes_total: e.bytes_total,
            path: e.path,
            message: e.message,
        }
    }
}

impl From<svc::AccountState> for AccountState {
    fn from(s: svc::AccountState) -> Self {
        match s {
            svc::AccountState::Created => Self::Created,
            svc::AccountState::Ready => Self::Ready,
            svc::AccountState::NeedsPairing => Self::NeedsPairing,
        }
    }
}

impl From<svc::AccountEvent> for AccountEvent {
    fn from(e: svc::AccountEvent) -> Self {
        match e {
            svc::AccountEvent::ListAdvanced { seq } => Self::ListAdvanced { seq },
            svc::AccountEvent::SelfRemoved => Self::SelfRemoved,
            svc::AccountEvent::TrustReset { devices } => Self::TrustReset { devices },
            svc::AccountEvent::HistoryReset { backup } => Self::HistoryReset { backup },
            svc::AccountEvent::SessionExpired => Self::SessionExpired,
        }
    }
}
