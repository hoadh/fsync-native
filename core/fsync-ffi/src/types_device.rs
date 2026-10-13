//! Records and enums: devices, platform, session tokens.

use fsync_core::service as svc;
use fsync_proto as proto;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Platform {
    Mac,
    Win,
    Cli,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DeviceStatus {
    Offline,
    Lan,
    Relay,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct SessionTokens {
    pub access: String,
    pub refresh: String,
    pub account_id: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Device {
    pub node_id: String,
    pub name: String,
    pub platform: Platform,
    pub status: DeviceStatus,
    pub online: bool,
    pub last_seen_ms: u64,
    pub is_self: bool,
}

impl From<proto::Platform> for Platform {
    fn from(p: proto::Platform) -> Self {
        match p {
            proto::Platform::Mac => Self::Mac,
            proto::Platform::Win => Self::Win,
            proto::Platform::Cli => Self::Cli,
        }
    }
}

impl From<Platform> for proto::Platform {
    fn from(p: Platform) -> Self {
        match p {
            Platform::Mac => Self::Mac,
            Platform::Win => Self::Win,
            Platform::Cli => Self::Cli,
        }
    }
}

impl From<svc::DeviceStatus> for DeviceStatus {
    fn from(s: svc::DeviceStatus) -> Self {
        match s {
            svc::DeviceStatus::Offline => Self::Offline,
            svc::DeviceStatus::Lan => Self::Lan,
            svc::DeviceStatus::Relay => Self::Relay,
        }
    }
}

impl From<svc::SessionTokens> for SessionTokens {
    fn from(t: svc::SessionTokens) -> Self {
        Self {
            access: t.access,
            refresh: t.refresh,
            account_id: t.account_id,
        }
    }
}

impl From<SessionTokens> for svc::SessionTokens {
    fn from(t: SessionTokens) -> Self {
        Self {
            access: t.access,
            refresh: t.refresh,
            account_id: t.account_id,
        }
    }
}

impl From<svc::Device> for Device {
    fn from(d: svc::Device) -> Self {
        Self {
            node_id: d.node_id,
            name: d.name,
            platform: d.platform.into(),
            status: d.status.into(),
            online: d.online,
            last_seen_ms: d.last_seen_ms,
            is_self: d.is_self,
        }
    }
}
