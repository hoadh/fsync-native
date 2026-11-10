//! Per-peer ordering of clip sends once `publish` no longer waits for slow peers.
//!
//! `publish` returns after a short wait and lets slow peers finish in the background, so the
//! attempt for clip N to a peer may still run while clip N+1 is published. The receiver applies
//! every offer it accepts as the newest clipboard (no ordering check), so N must never reach the
//! peer after N+1. Each peer has one lane: sends to it run one at a time, and a send that gets its
//! turn after a newer clip was queued for that peer is skipped (latest wins). At most one attempt
//! per peer is on the wire, so a dead peer never accumulates dials either.

use fsync_proto::NodeId;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::{Mutex as AsyncMutex, MutexGuard};

#[derive(Default)]
struct Lane {
    /// Number of the newest clip queued for this peer.
    latest: AtomicU64,
    /// Held for the whole send attempt.
    turn: AsyncMutex<()>,
}

/// One lane per paired peer (the map only grows with the paired device list).
#[derive(Default)]
pub(crate) struct PeerSendLanes(Mutex<HashMap<NodeId, Arc<Lane>>>);

/// A clip's place in a peer's lane. Taken in publish order, before the send task starts.
pub(crate) struct LaneTicket {
    lane: Arc<Lane>,
    seq: u64,
}

impl PeerSendLanes {
    pub(crate) fn ticket(&self, peer: NodeId) -> LaneTicket {
        let lane = self
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(peer)
            .or_default()
            .clone();
        let seq = lane.latest.fetch_add(1, Ordering::SeqCst) + 1;
        LaneTicket { lane, seq }
    }
}

impl LaneTicket {
    /// An older send to this peer is still running (with apps publishing one clip at a time,
    /// that is an attempt that outlived its `publish`: the peer is slow or offline).
    pub(crate) fn busy(&self) -> bool {
        self.lane.turn.try_lock().is_err()
    }

    /// Waits until no other send to this peer runs. `None`: a newer clip was queued for the peer
    /// meanwhile, so this one must not be sent (it would overwrite the newer one there).
    pub(crate) async fn turn(&self) -> Option<MutexGuard<'_, ()>> {
        let guard = self.lane.turn.lock().await;
        (self.lane.latest.load(Ordering::SeqCst) == self.seq).then_some(guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn peer(c: char) -> NodeId {
        c.to_string().repeat(64).parse().unwrap()
    }

    #[tokio::test]
    async fn older_clip_waiting_for_its_turn_is_skipped() {
        let lanes = PeerSendLanes::default();
        let (n1, n2) = (lanes.ticket(peer('a')), lanes.ticket(peer('a')));
        assert!(n1.turn().await.is_none(), "n1 was superseded by n2");
        assert!(n2.turn().await.is_some());
    }

    #[tokio::test]
    async fn sends_to_one_peer_run_one_at_a_time_other_peers_do_not_wait() {
        let lanes = PeerSendLanes::default();
        let n1 = lanes.ticket(peer('a'));
        assert!(!n1.busy());
        let held = n1.turn().await.expect("latest");
        let n2 = lanes.ticket(peer('a'));
        assert!(n2.busy(), "n1's attempt still runs");
        let other = lanes.ticket(peer('b'));
        assert!(other.turn().await.is_some(), "peer b has its own lane");
        let blocked = tokio::time::timeout(Duration::from_millis(50), n2.turn()).await;
        assert!(blocked.is_err(), "n2 must wait for n1's attempt");
        drop(held);
        assert!(n2.turn().await.is_some());
    }
}
