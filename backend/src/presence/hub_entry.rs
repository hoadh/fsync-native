//! Per-socket bookkeeping shared by the in-memory hub.

use super::hub_api::{ConnId, Outbound};
use crate::util::now_ms;
use fsync_proto::{
    NodeId,
    presence::{Addrs, PeerInfo, ServerMsg},
};
use std::collections::HashMap;
use tokio::sync::mpsc;
use uuid::Uuid;

pub struct Entry {
    pub conn: ConnId,
    pub sid: Uuid,
    pub tx: Option<mpsc::Sender<Outbound>>,
    pub pending: bool,
    pub addrs: Addrs,
    pub last_seen_ms: u64,
}

impl Entry {
    pub fn info(&self, node: NodeId) -> PeerInfo {
        PeerInfo {
            node_id: node,
            online: self.tx.is_some(),
            addrs: self.addrs.clone(),
            last_seen_ms: self.last_seen_ms,
        }
    }

    /// Ask the socket task to close; dropping the sender ends it even if the queue is full.
    pub fn close(&mut self, code: u16, reason: &'static str) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.try_send(Outbound::Close(code, reason));
        }
    }
}

pub type Account = HashMap<NodeId, Entry>;

pub fn now_u64() -> u64 {
    u64::try_from(now_ms()).unwrap_or(0)
}

/// Deliver `msg` to every online entry except `skip`; entries whose queue is full or closed
/// are dropped (their socket task ends and the client reconnects).
pub fn broadcast(
    acct: &mut Account,
    msg: &ServerMsg,
    skip: Option<&NodeId>,
    include_pending: bool,
) {
    for (node, e) in acct.iter_mut() {
        if skip == Some(node) || (e.pending && !include_pending) {
            continue;
        }
        if let Some(tx) = &e.tx
            && tx.try_send(Outbound::Msg(msg.clone())).is_err()
        {
            e.tx = None;
        }
    }
}
