//! Per-peer token bucket for incoming offers.

use fsync_proto::NodeId;
use std::{collections::HashMap, sync::Mutex, time::Instant};

/// Sustained offers per second per peer.
pub const OFFERS_PER_SEC: f64 = 20.0;
/// Burst size.
pub const OFFER_BURST: f64 = 20.0;

#[derive(Default)]
pub struct PeerRate(Mutex<HashMap<NodeId, (f64, Instant)>>);

impl PeerRate {
    /// Take a token for `peer`; false when it is over its rate.
    pub fn allow(&self, peer: NodeId) -> bool {
        let Ok(mut g) = self.0.lock() else {
            return false;
        };
        let now = Instant::now();
        let e = g.entry(peer).or_insert((OFFER_BURST, now));
        let refill = now.duration_since(e.1).as_secs_f64() * OFFERS_PER_SEC;
        e.0 = (e.0 + refill).min(OFFER_BURST);
        e.1 = now;
        if e.0 >= 1.0 {
            e.0 -= 1.0;
            true
        } else {
            false
        }
    }
}
