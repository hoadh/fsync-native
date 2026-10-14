//! History page types crossing the service boundary.

use fsync_proto::ClipKind;

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub struct HistoryPage {
    pub items: Vec<HistoryItem>,
    pub next_cursor: Option<String>,
}
