//! Address discovery: backend presence feeds a `MemoryLookup`; mDNS finds LAN peers (ADR 007).

use super::convert::endpoint_id;
use crate::{error::Result, trust::TrustManager};
use fsync_proto::{NodeId, presence::PeerInfo};
use iroh::{EndpointAddr, RelayUrl, address_lookup::MemoryLookup};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

const MAX_KNOWN: usize = 128;

#[derive(Clone)]
pub struct Discovery {
    lookup: MemoryLookup,
    trust: Arc<TrustManager>,
    /// Last presence entry per node, so a device paired later still gets its addresses.
    known: Arc<Mutex<HashMap<NodeId, PeerInfo>>>,
}

impl Discovery {
    pub fn new(lookup: MemoryLookup, trust: Arc<TrustManager>) -> Self {
        Self {
            lookup,
            trust,
            known: Arc::default(),
        }
    }

    /// Seed or refresh a peer's addresses (e.g. the relay URL from a pairing lookup).
    pub fn hint(&self, node: &NodeId, relay: Option<&str>, direct: &[SocketAddr]) -> Result<()> {
        let mut addr = EndpointAddr::new(endpoint_id(node)?);
        if let Some(r) = relay.and_then(|r| r.parse::<RelayUrl>().ok()) {
            addr = addr.with_relay_url(r);
        }
        for d in direct {
            addr = addr.with_ip_addr(*d);
        }
        self.lookup.set_endpoint_info(addr);
        Ok(())
    }

    /// Apply one presence entry. Only paired devices reach the lookup table.
    pub fn apply_peer(&self, p: &PeerInfo) {
        if let Ok(mut k) = self.known.lock()
            && (k.len() < MAX_KNOWN || k.contains_key(&p.node_id))
        {
            k.insert(p.node_id, p.clone());
        }
        self.feed(p);
    }

    pub fn apply_peers(&self, peers: &[PeerInfo]) {
        peers.iter().for_each(|p| self.apply_peer(p));
    }

    /// Re-feed everything known (call after the pinned list changed).
    pub fn refresh(&self) {
        let all: Vec<PeerInfo> = self
            .known
            .lock()
            .map(|k| k.values().cloned().collect())
            .unwrap_or_default();
        all.iter().for_each(|p| self.feed(p));
    }

    fn feed(&self, p: &PeerInfo) {
        if !self.trust.is_trusted(&p.node_id) {
            return;
        }
        let Ok(id) = endpoint_id(&p.node_id) else {
            return;
        };
        if p.online {
            if let Err(e) = self.hint(&p.node_id, p.addrs.relay_url.as_deref(), &p.addrs.direct) {
                tracing::debug!("skipping peer addrs: {e}");
            }
        } else {
            self.lookup.remove_endpoint_info(id);
        }
    }
}
