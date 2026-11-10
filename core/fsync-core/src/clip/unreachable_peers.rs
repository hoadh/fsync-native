//! Peers whose last clip send failed on the network (dial or ack timed out).
//!
//! `publish` waits for every peer, and the apps run one publish at a time, so an offline device
//! would add its full dial cost (about 7 s) to every clip. A peer marked here is skipped (reported
//! as failed at once) for `BACKOFF`; the first publish after that probes it again at full cost.
//! A successful send to the peer, or any offer received from it, clears the mark.

use fsync_proto::NodeId;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

/// How long a peer that failed to answer is skipped before it is probed again.
pub(crate) const BACKOFF: Duration = Duration::from_secs(20);

#[derive(Default)]
pub struct UnreachablePeers(Mutex<HashMap<NodeId, Instant>>);

impl UnreachablePeers {
    /// True while `peer` is inside its back-off window. An expired mark is dropped.
    pub(crate) fn is_skipped(&self, peer: &NodeId, now: Instant) -> bool {
        let Ok(mut m) = self.0.lock() else {
            return false;
        };
        match m.get(peer) {
            Some(at) if now.saturating_duration_since(*at) < BACKOFF => true,
            Some(_) => {
                m.remove(peer);
                false
            }
            None => false,
        }
    }

    pub(crate) fn mark(&self, peer: NodeId, now: Instant) {
        if let Ok(mut m) = self.0.lock() {
            m.insert(peer, now);
        }
    }

    pub(crate) fn clear(&self, peer: &NodeId) {
        if let Ok(mut m) = self.0.lock() {
            m.remove(peer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(b: u8) -> NodeId {
        NodeId::from_signing_key(&ed25519_dalek::SigningKey::from_bytes(&[b; 32]))
    }

    #[test]
    fn mark_skips_until_backoff_then_clears() {
        let u = UnreachablePeers::default();
        let (a, b) = (id(1), id(2));
        let t0 = Instant::now();
        assert!(!u.is_skipped(&a, t0));
        u.mark(a, t0);
        assert!(u.is_skipped(&a, t0 + BACKOFF / 2));
        assert!(!u.is_skipped(&b, t0));
        assert!(!u.is_skipped(&a, t0 + BACKOFF));
        u.mark(a, t0);
        u.clear(&a);
        assert!(!u.is_skipped(&a, t0));
    }
}
