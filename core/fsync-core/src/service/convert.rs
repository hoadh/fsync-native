//! Mapping between core types and service types.

use super::{
    error::{FsyncError, SvcResult},
    types::{ClipItem, DeviceStatus, FileInfo, RepInfo},
};
use crate::{events::PathState, trust::TrustManager};
use fsync_proto::{ClipOffer, NodeId};
use uuid::Uuid;

impl From<PathState> for DeviceStatus {
    fn from(p: PathState) -> Self {
        match p {
            PathState::Offline => Self::Offline,
            PathState::Lan => Self::Lan,
            PathState::Relay => Self::Relay,
        }
    }
}

pub(super) fn parse_node(s: &str) -> SvcResult<NodeId> {
    s.trim()
        .parse()
        .map_err(|_| FsyncError::invalid("node id must be 64 hex characters"))
}

pub(super) fn parse_uuid(s: &str) -> SvcResult<Uuid> {
    Uuid::parse_str(s.trim()).map_err(|_| FsyncError::invalid("malformed item id"))
}

/// Name of a paired device, or a short id when it is not (or no longer) listed.
pub(super) fn device_name(trust: &TrustManager, id: &NodeId) -> String {
    trust
        .list()
        .and_then(|l| l.devices.into_iter().find(|d| d.node_id == *id))
        .map(|d| d.name)
        .unwrap_or_else(|| id.to_string().chars().take(8).collect())
}

pub(super) fn clip_item(trust: &TrustManager, from: &NodeId, o: &ClipOffer) -> ClipItem {
    let total =
        o.reps.iter().map(|r| r.size).sum::<u64>() + o.files.iter().map(|f| f.size).sum::<u64>();
    ClipItem {
        item_id: o.item_id.to_string(),
        from: from.to_string(),
        from_name: device_name(trust, from),
        kind: o.kind,
        ts_ms: o.ts_ms,
        preview: o.preview.clone(),
        reps: o
            .reps
            .iter()
            .map(|r| RepInfo {
                mime: r.mime.clone(),
                size: r.size,
            })
            .collect(),
        files: o
            .files
            .iter()
            .map(|f| FileInfo {
                file_id: f.file_id,
                name: f.name.clone(),
                rel_path: f.rel_path.clone(),
                size: f.size,
            })
            .collect(),
        total_size: total,
        push: o.push,
    }
}

/// Lowercase hex of the offered head's hash: the id apps pass back to confirm / decline.
pub(super) fn reset_id(head: &[u8; 32]) -> String {
    use std::fmt::Write;
    head.iter().fold(String::with_capacity(64), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Inverse of `reset_id`: exactly 64 hex digits. Anything else can never match a pending
/// reset, so it is `NotFound` like any other unknown id.
pub(super) fn parse_reset_id(s: &str) -> SvcResult<[u8; 32]> {
    let mut out = [0u8; 32];
    if s.len() != out.len() * 2 {
        return Err(FsyncError::NotFound);
    }
    let digit = |b: u8| {
        char::from(b)
            .to_digit(16)
            .and_then(|d| u8::try_from(d).ok())
    };
    let (pairs, _) = s.as_bytes().as_chunks::<2>();
    for (byte, [hi, lo]) in out.iter_mut().zip(pairs) {
        let (Some(hi), Some(lo)) = (digit(*hi), digit(*lo)) else {
            return Err(FsyncError::NotFound);
        };
        *byte = (hi << 4) | lo;
    }
    Ok(out)
}
