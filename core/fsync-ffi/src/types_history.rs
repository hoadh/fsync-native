//! Records and enums: history pages and settings.

use crate::types_clip::ClipKind;
use fsync_core::service as svc;

#[derive(Clone, Debug, uniffi::Record)]
pub struct HistoryItem {
    pub id: String,
    pub outgoing: bool,
    pub peer: Option<String>,
    pub kind: ClipKind,
    pub created_ms: u64,
    pub total_size: u64,
    pub preview: Option<String>,
    pub pinned: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct HistoryPage {
    pub items: Vec<HistoryItem>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ClipMode {
    Auto,
    Manual,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct Settings {
    pub clip_mode: ClipMode,
    pub retention_days: u32,
    pub retention_bytes: u64,
    pub max_clip_bytes: u64,
    pub denylist: Vec<String>,
    pub auto_accept_files: bool,
}

impl From<svc::HistoryPage> for HistoryPage {
    fn from(p: svc::HistoryPage) -> Self {
        Self {
            items: p
                .items
                .into_iter()
                .map(|i| HistoryItem {
                    id: i.id,
                    outgoing: i.outgoing,
                    peer: i.peer,
                    kind: i.kind.into(),
                    created_ms: i.created_ms,
                    total_size: i.total_size,
                    preview: i.preview,
                    pinned: i.pinned,
                })
                .collect(),
            next_cursor: p.next_cursor,
        }
    }
}

impl From<svc::Settings> for Settings {
    fn from(s: svc::Settings) -> Self {
        Self {
            clip_mode: match s.clip_mode {
                svc::ClipMode::Auto => ClipMode::Auto,
                svc::ClipMode::Manual => ClipMode::Manual,
            },
            retention_days: s.retention_days,
            retention_bytes: s.retention_bytes,
            max_clip_bytes: s.max_clip_bytes,
            denylist: s.denylist,
            auto_accept_files: s.auto_accept_files,
        }
    }
}

impl From<Settings> for svc::Settings {
    fn from(s: Settings) -> Self {
        Self {
            clip_mode: match s.clip_mode {
                ClipMode::Auto => svc::ClipMode::Auto,
                ClipMode::Manual => svc::ClipMode::Manual,
            },
            retention_days: s.retention_days,
            retention_bytes: s.retention_bytes,
            max_clip_bytes: s.max_clip_bytes,
            denylist: s.denylist,
            auto_accept_files: s.auto_accept_files,
        }
    }
}
