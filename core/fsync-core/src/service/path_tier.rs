//! Which kind of path (lan | direct | relay) a paired device is reached over right now.

use super::{FsyncService, convert::parse_node, inner::lock};
use crate::net::PathTier;

impl FsyncService {
    /// Best path tier across the live connections to `node_id`. `None` when the engine is not
    /// running, the id is malformed, or no connection is up (a finished transfer's connection may
    /// already have closed, so sample right at completion). Core-only: not part of the FFI.
    pub fn peer_path_tier(&self, node_id: &str) -> Option<PathTier> {
        let id = parse_node(node_id).ok()?;
        let engine = lock(&self.inner.live).ok()?.engine.clone()?;
        engine.node().peer_path_tier(&id)
    }
}
