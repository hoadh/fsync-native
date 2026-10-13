//! Presence hub contract: what the socket tasks and HTTP routes may ask of the hub.

use fsync_proto::{
    NodeId,
    presence::{Addrs, PeerInfo, ServerMsg},
};
use tokio::sync::mpsc;
use uuid::Uuid;

pub type ConnId = u64;

/// What the hub pushes to a socket task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outbound {
    Msg(ServerMsg),
    /// Send a close frame and end the socket.
    Close(u16, &'static str),
}

pub const CLOSE_UNAUTHORIZED: u16 = 4401;

pub trait PresenceHub: Send + Sync {
    /// Register a socket. A newer connection for the same node replaces (closes) the older one.
    /// `pending` = node is not yet in the device list (pairing): it is visible to others but
    /// receives no peer data. `sid` is the access token's session (refresh family) id.
    fn connect(
        &self,
        account: Uuid,
        node: NodeId,
        pending: bool,
        sid: Uuid,
        tx: mpsc::Sender<Outbound>,
    ) -> ConnId;
    fn set_addrs(&self, account: Uuid, node: NodeId, conn: ConnId, addrs: Addrs);
    fn disconnect(&self, account: Uuid, node: NodeId, conn: ConnId);
    /// Peers of `me`: every listed device (online or not) plus any other online node.
    fn snapshot(&self, account: Uuid, me: &NodeId, listed: &[NodeId]) -> Vec<PeerInfo>;
    /// Notify the account's sockets. Connected nodes absent from `listed` (and not pending
    /// pairing nodes) are closed directly and receive nothing further.
    fn device_list_changed(&self, account: Uuid, seq: u64, listed: &[NodeId]);
    /// Close the sockets opened with session `sid` (logout / family revoked).
    fn close_session(&self, account: Uuid, sid: Uuid);
    /// Close all sockets of an account (account deleted).
    fn close_account(&self, account: Uuid);
}
