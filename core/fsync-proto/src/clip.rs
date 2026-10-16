//! clip/2 frames: sender announces, receiver pulls.

use crate::names::validate_file_name;
pub use crate::names::validate_rel_path;
use crate::{error::ProtoError, node_id::NodeId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// clip/2: one bi stream per offer. The sender writes one `ClipOffer` frame and the receiver
/// answers with one `CLIP_ACK_*` byte, so "delivered" means the receiver processed the offer
/// (clip/1 only had a transport ack on a uni stream and could not tell a refusal).
pub const ALPN_CLIP: &[u8] = b"fsync/clip/2";
/// Receiver validated and kept the offer (a duplicate also counts).
pub const CLIP_ACK_ACCEPTED: u8 = 1;
/// Receiver dropped the offer: per-peer rate limit.
pub const CLIP_ACK_RATE_LIMITED: u8 = 2;
/// Receiver dropped the offer: invalid, too large or unreadable.
pub const CLIP_ACK_REJECTED: u8 = 3;
pub const ALPN_FILE: &[u8] = b"fsync/file/1";
pub const ALPN_PAIR: &[u8] = b"fsync/pair/1";
pub const CLIP_VERSION: u8 = 1;
/// Total inline payload budget per offer.
pub const MAX_INLINE_TOTAL: usize = 256 * 1024;
pub const MAX_PREVIEW_CHARS: usize = 200;
pub const MAX_FILES: usize = 10_000;
pub const MAX_REPS: usize = 16;
pub const MAX_MIME_CHARS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipKind {
    Text,
    Rich,
    Image,
    Files,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepMeta {
    pub mime: String,
    pub size: u64,
    /// blake3 of the rep bytes.
    pub hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMeta {
    pub file_id: u32,
    pub name: String,
    pub rel_path: String,
    pub size: u64,
    pub hash: [u8; 32],
    pub mtime_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipOffer {
    pub v: u8,
    pub item_id: Uuid,
    pub origin: NodeId,
    pub ts_ms: u64,
    pub kind: ClipKind,
    pub reps: Vec<RepMeta>,
    /// One entry per rep; `Some` when the bytes ride inline.
    pub inline: Vec<Option<Vec<u8>>>,
    pub files: Vec<FileMeta>,
    pub preview: Option<String>,
    /// "Send to..." intent (kind = Files): receiver may auto-accept per its setting.
    pub push: bool,
    /// Concealed/secret content (password manager, ...): receivers must not keep it in
    /// history and should not leave blobs of it behind.
    pub sensitive: bool,
}

impl ClipOffer {
    /// Receiver-side sanity checks; run before storing or acting on a peer's offer.
    pub fn validate(&self) -> Result<(), ProtoError> {
        if self.v != CLIP_VERSION {
            return Err(ProtoError::UnsupportedVersion(self.v));
        }
        if self.reps.len() > MAX_REPS {
            return Err(ProtoError::InvalidOffer("too many reps"));
        }
        if self.files.len() > MAX_FILES {
            return Err(ProtoError::InvalidOffer("too many files"));
        }
        if self
            .reps
            .iter()
            .any(|r| r.mime.chars().count() > MAX_MIME_CHARS)
        {
            return Err(ProtoError::InvalidOffer("mime too long"));
        }
        if self.inline.len() != self.reps.len() {
            return Err(ProtoError::InvalidOffer("inline/reps length mismatch"));
        }
        let mut total = 0usize;
        for (rep, bytes) in self.reps.iter().zip(&self.inline) {
            let Some(bytes) = bytes else { continue };
            total = total.saturating_add(bytes.len());
            if u64::try_from(bytes.len()).ok() != Some(rep.size) {
                return Err(ProtoError::InvalidOffer("inline size mismatch"));
            }
            if blake3::hash(bytes).as_bytes() != &rep.hash {
                return Err(ProtoError::InvalidOffer("inline hash mismatch"));
            }
        }
        if total > MAX_INLINE_TOTAL {
            return Err(ProtoError::InvalidOffer("inline budget exceeded"));
        }
        if self
            .preview
            .as_ref()
            .is_some_and(|p| p.chars().count() > MAX_PREVIEW_CHARS)
        {
            return Err(ProtoError::InvalidOffer("preview too long"));
        }
        if (self.kind == ClipKind::Files) == self.files.is_empty() {
            return Err(ProtoError::InvalidOffer(
                "files must be present iff kind is Files",
            ));
        }
        let mut seen = std::collections::HashSet::with_capacity(self.files.len());
        for f in &self.files {
            if !seen.insert(f.file_id) {
                return Err(ProtoError::InvalidOffer("duplicate file_id"));
            }
            validate_file_name(&f.name)?;
            validate_rel_path(&f.rel_path)?;
        }
        Ok(())
    }
}
