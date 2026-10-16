//! Cached outbound connections to paired devices (one per peer + ALPN).

use super::{discovery::Discovery, gate::Gate};
use crate::{
    error::{CoreError, Result},
    trust::TrustManager,
};
use fsync_proto::NodeId;
use iroh::{Endpoint, endpoint::Connection};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

type DialLocks = Mutex<HashMap<(NodeId, &'static [u8]), Arc<tokio::sync::Mutex<()>>>>;
type ConnMap = HashMap<(NodeId, &'static [u8]), Connection>;

const DIAL_TIMEOUT: Duration = Duration::from_secs(10);
const FIRST_ATTEMPT: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct Dialer {
    pub(super) ep: Endpoint,
    pub(super) gate: Arc<Gate>,
    pub(super) trust: Arc<TrustManager>,
    discovery: Discovery,
    conns: Arc<Mutex<ConnMap>>,
    /// Single-flight: concurrent callers for one peer+ALPN share a single dial.
    dials: Arc<DialLocks>,
}

impl Dialer {
    pub(super) fn new(
        ep: Endpoint,
        gate: Arc<Gate>,
        trust: Arc<TrustManager>,
        discovery: Discovery,
    ) -> Self {
        Self {
            ep,
            gate,
            trust,
            discovery,
            conns: Arc::default(),
            dials: Arc::default(),
        }
    }

    /// Open (or reuse) a connection to a trusted peer. Addresses come from the lookup.
    pub async fn connect(&self, peer: &NodeId, alpn: &'static [u8]) -> Result<Connection> {
        self.connect_reporting_reuse(peer, alpn)
            .await
            .map(|(c, _)| c)
    }

    /// Like `connect`, also telling whether the connection came from the cache. A cached one may
    /// be dead without being closed yet (peer crashed), so callers can use a short first attempt.
    pub async fn connect_reporting_reuse(
        &self,
        peer: &NodeId,
        alpn: &'static [u8],
    ) -> Result<(Connection, bool)> {
        if !self.trust.is_trusted(peer) {
            return Err(CoreError::NotTrusted(peer.to_string()));
        }
        if let Some(c) = self.cached(peer, alpn) {
            return Ok((c, true));
        }
        let lock = {
            let mut g = self
                .dials
                .lock()
                .map_err(|_| CoreError::Net("dialer lock poisoned".into()))?;
            // Idle entries (nobody holds or waits) are pruned so the map stays small.
            g.retain(|_, m| Arc::strong_count(m) > 1);
            g.entry((*peer, alpn)).or_default().clone()
        };
        let _one = lock.lock().await;
        if let Some(c) = self.cached(peer, alpn) {
            return Ok((c, true));
        }
        let conn = self.dial_with_recovery(peer, alpn).await?;
        self.gate.track_outbound(&conn);
        if let Ok(mut g) = self.conns.lock() {
            g.insert((*peer, alpn), conn.clone());
        }
        Ok((conn, false))
    }

    /// First attempt is short. If it stalls, the peer probably restarted (same id, new port)
    /// while our older connections for this ALPN still look alive: drop them so iroh forgets the dead
    /// path and re-reads the current addresses, then retry for the rest of `DIAL_TIMEOUT`.
    async fn dial_with_recovery(&self, peer: &NodeId, alpn: &'static [u8]) -> Result<Connection> {
        let started = tokio::time::Instant::now();
        let started_std = std::time::Instant::now();
        let marks = self.gate.inbound_rx_marks(peer);
        let addr = self.discovery.addr_for(peer)?;
        match tokio::time::timeout(FIRST_ATTEMPT, self.ep.connect(addr, alpn)).await {
            Ok(res) => return res.map_err(CoreError::net),
            Err(_) => {
                self.forget(peer, alpn);
                self.gate.close_stale(peer, alpn, started_std);
                self.gate.close_inbound_except_active_pulls(peer, &marks);
            }
        }
        let left = DIAL_TIMEOUT.saturating_sub(started.elapsed());
        let addr = self.discovery.addr_for(peer)?;
        tokio::time::timeout(left, self.ep.connect(addr, alpn))
            .await
            .map_err(|_| CoreError::Net("dial timed out".into()))?
            .map_err(CoreError::net)
    }

    /// Drop a cached connection that turned out to be dead.
    pub fn forget(&self, peer: &NodeId, alpn: &'static [u8]) {
        if let Ok(mut g) = self.conns.lock() {
            g.remove(&(*peer, alpn));
        }
    }

    /// A reused connection looked dead to one caller: stop handing it out, but only if it is still
    /// the cached one (a concurrent caller may already have replaced it). It is closed only when it
    /// received nothing at all since `rx_before` (byte count taken before the caller's request):
    /// streams in flight on a healthy shared connection always receive data or acks, so those
    /// fetches are never cut off. A silent one is dead and would keep iroh routing to its path.
    pub fn abandon(&self, peer: &NodeId, alpn: &'static [u8], conn: &Connection, rx_before: u64) {
        if let Ok(mut g) = self.conns.lock()
            && g.get(&(*peer, alpn))
                .is_some_and(|c| c.stable_id() == conn.stable_id())
        {
            g.remove(&(*peer, alpn));
        }
        if conn.stats().udp_rx.bytes == rx_before {
            conn.close(0u32.into(), b"dead path");
        }
    }

    fn cached(&self, peer: &NodeId, alpn: &'static [u8]) -> Option<Connection> {
        let g = self.conns.lock().ok()?;
        g.get(&(*peer, alpn))
            .filter(|c| c.close_reason().is_none())
            .cloned()
    }
}
