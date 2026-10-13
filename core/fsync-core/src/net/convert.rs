//! `NodeId` (fsync-proto) <-> iroh `EndpointId`: same 32 ed25519 public-key bytes.

use crate::error::{CoreError, Result};
use fsync_proto::NodeId;
use iroh::{EndpointId, endpoint::Connection};

pub fn endpoint_id(n: &NodeId) -> Result<EndpointId> {
    EndpointId::from_bytes(n.as_bytes()).map_err(|_| CoreError::Invalid("invalid node id"))
}

pub fn node_id(e: &EndpointId) -> NodeId {
    NodeId(*e.as_bytes())
}

/// Authenticated remote identity of a connection (from the TLS handshake).
pub fn node_id_of(conn: &Connection) -> NodeId {
    node_id(&conn.remote_id())
}
