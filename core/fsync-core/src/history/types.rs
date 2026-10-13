//! Row types of the history store.

use crate::blobs::Hash;
use fsync_proto::{ClipKind, NodeId};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Out,
    In,
}

impl Direction {
    pub(super) fn code(self) -> i64 {
        match self {
            Self::Out => 0,
            Self::In => 1,
        }
    }
    pub(super) fn from_code(c: i64) -> Self {
        if c == 0 { Self::Out } else { Self::In }
    }
}

pub(super) fn kind_code(k: ClipKind) -> i64 {
    match k {
        ClipKind::Text => 0,
        ClipKind::Rich => 1,
        ClipKind::Image => 2,
        ClipKind::Files => 3,
    }
}

pub(super) fn kind_from_code(c: i64) -> ClipKind {
    match c {
        0 => ClipKind::Text,
        1 => ClipKind::Rich,
        2 => ClipKind::Image,
        _ => ClipKind::Files,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepRow {
    pub idx: u32,
    pub mime: String,
    pub size: u64,
    pub hash: Hash,
    /// The bytes are in the blob store.
    pub present: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileState {
    /// Announced; not saved by the user yet.
    Offered,
    /// Placed at `local_path`.
    Saved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRow {
    pub file_id: u32,
    pub name: String,
    pub rel_path: String,
    pub size: u64,
    pub hash: Hash,
    pub local_path: Option<String>,
    pub state: FileState,
}

/// An item to record. `sensitive` items are refused by `History::record`.
#[derive(Clone, Debug)]
pub struct NewItem {
    pub id: Uuid,
    pub direction: Direction,
    /// Origin (received) or the single target (sent); `None` when broadcast.
    pub peer: Option<NodeId>,
    pub kind: ClipKind,
    pub created_ms: u64,
    pub preview: Option<String>,
    pub sensitive: bool,
    pub reps: Vec<RepRow>,
    pub files: Vec<FileRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: Uuid,
    pub direction: Direction,
    pub peer: Option<NodeId>,
    pub kind: ClipKind,
    pub created_ms: u64,
    pub total_size: u64,
    pub preview: Option<String>,
    pub pinned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemDetail {
    pub item: Item,
    pub reps: Vec<RepRow>,
    pub files: Vec<FileRow>,
}

/// Position after the last row of a page: rows are ordered by `(created_ms, id)` descending.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub created_ms: u64,
    pub id: Uuid,
}

#[derive(Clone, Debug, Default)]
pub struct Page {
    pub items: Vec<Item>,
    /// `Some` when more rows follow.
    pub next: Option<Cursor>,
}
