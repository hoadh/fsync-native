//! file/1 frames: receiver opens a bi stream per fetch; verified byte stream follows `FetchResp`.

use crate::{clip::CLIP_VERSION, error::ProtoError};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Offsets must be aligned to a 16 KiB chunk group.
pub const CHUNK_GROUP: u64 = 16 * 1024;

/// After an `Ok` response the sender writes, in order (all raw, no framing):
/// 1. unless `no_cvs` or the file has <= 1 group: `ceil(size/CHUNK_GROUP)` 32-byte blake3
///    chaining values, one per group (non-root; their tree merge must equal `FetchResp::hash`);
/// 2. the data bytes from `offset`, up to `len` (or the end).
///
/// Each group is checked against its chaining value before it is used.
pub const CV_LEN: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FetchTarget {
    /// Index into `ClipOffer::reps`.
    Rep(u32),
    /// `FileMeta::file_id`.
    File(u32),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchReq {
    pub v: u8,
    pub item_id: Uuid,
    pub target: FetchTarget,
    pub offset: u64,
    /// Max data bytes to send from `offset` (0 = until the end). Used by ranged reads.
    pub len: u64,
    /// Skip the chunk-group hash list (requester already holds the verified list).
    pub no_cvs: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FetchStatus {
    Ok,
    NotFound,
    /// Source file changed since it was announced.
    Changed,
    Denied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchResp {
    pub status: FetchStatus,
    pub size: u64,
    pub hash: [u8; 32],
}

impl FetchReq {
    pub fn new(item_id: Uuid, target: FetchTarget, offset: u64) -> Self {
        Self {
            v: CLIP_VERSION,
            item_id,
            target,
            offset,
            len: 0,
            no_cvs: false,
        }
    }

    /// Ranged request: at most `len` bytes from `offset`; list skipped when already held.
    pub fn ranged(mut self, len: u64, have_cvs: bool) -> Self {
        self.len = len;
        self.no_cvs = have_cvs;
        self
    }

    pub fn validate(&self) -> Result<(), ProtoError> {
        if self.v != CLIP_VERSION {
            return Err(ProtoError::UnsupportedVersion(self.v));
        }
        if !self.offset.is_multiple_of(CHUNK_GROUP) {
            return Err(ProtoError::MisalignedOffset);
        }
        Ok(())
    }
}
