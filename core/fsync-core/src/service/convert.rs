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
