//! Cached outbound connections to paired devices (one per peer + ALPN).

use super::{convert::endpoint_id, gate::Gate};
use crate::{
    error::{CoreError, Result},
    trust::TrustManager,
};
use fsync_proto::NodeId;
use iroh::{Endpoint, EndpointAddr, endpoint::Connection};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

type DialLocks = Mutex<HashMap<(NodeId, &'static [u8]), Arc<tokio::sync::Mutex<()>>>>;
type ConnMap = HashMap<(NodeId, &'static [u8]), Connection>;

const DIAL_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct Dialer {
    pub(super) ep: Endpoint,
    pub(super) gate: Arc<Gate>,
    pub(super) trust: Arc<TrustManager>,
    conns: Arc<Mutex<ConnMap>>,
    /// Single-flight: concurrent callers for one peer+ALPN share a single dial.
    dials: Arc<DialLocks>,
}

impl Dialer {
    pub(super) fn new(ep: Endpoint, gate: Arc<Gate>, trust: Arc<TrustManager>) -> Self {
        Self {
            ep,
            gate,
            trust,
            conns: Arc::default(),
            dials: Arc::default(),
        }
    }

    /// Open (or reuse) a connection to a trusted peer. Addresses come from the lookup.
    pub async fn connect(&self, peer: &NodeId, alpn: &'static [u8]) -> Result<Connection> {
        if !self.trust.is_trusted(peer) {
            return Err(CoreError::NotTrusted(peer.to_string()));
        }
        if let Some(c) = self.cached(peer, alpn) {
            return Ok(c);
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
            return Ok(c);
        }
        let addr = EndpointAddr::new(endpoint_id(peer)?);
        let conn = tokio::time::timeout(DIAL_TIMEOUT, self.ep.connect(addr, alpn))
            .await
            .map_err(|_| CoreError::Net("dial timed out".into()))?
            .map_err(CoreError::net)?;
        self.gate.track(&conn);
        if let Ok(mut g) = self.conns.lock() {
            g.insert((*peer, alpn), conn.clone());
        }
        Ok(conn)
    }

    /// Drop a cached connection that turned out to be dead.
    pub fn forget(&self, peer: &NodeId, alpn: &'static [u8]) {
        if let Ok(mut g) = self.conns.lock() {
            g.remove(&(*peer, alpn));
        }
    }

    fn cached(&self, peer: &NodeId, alpn: &'static [u8]) -> Option<Connection> {
        let g = self.conns.lock().ok()?;
        g.get(&(*peer, alpn))
            .filter(|c| c.close_reason().is_none())
            .cloned()
    }
}
