//! `Node`: the running iroh endpoint with gated protocols and pairing entry points.

use super::{
    convert::endpoint_id,
    dialer::Dialer,
    discovery::Discovery,
    gate::Gate,
    node_addrs::{AddrFeed, addrs_of},
    node_builder::NodeBuilder,
    path_state::PathTracker,
};
use crate::{
    error::{CoreError, Result},
    events::{EventSink, PathState},
    identity::Identity,
    pair::{DeviceInfo, ListBackend, OpenPairing, PairSecret, PairingWindows, run_initiator},
    trust::TrustManager,
};
use fsync_proto::{ALPN_PAIR, NodeId, presence::Addrs};
use iroh::{Endpoint, EndpointAddr, endpoint::Connection, protocol::Router};
use std::{sync::Arc, time::Duration};

pub struct Node {
    pub discovery: Discovery,
    pub(super) identity: Identity,
    pub(super) trust: Arc<TrustManager>,
    pub(super) events: EventSink,
    pub(super) ep: Endpoint,
    pub(super) router: Router,
    pub(super) windows: Arc<PairingWindows>,
    pub(super) gate: Arc<Gate>,
    pub(super) paths: Arc<PathTracker>,
    pub(super) enforcer: tokio::task::JoinHandle<()>,
    pub(super) feed: AddrFeed,
    pub(super) feeder: tokio::task::JoinHandle<()>,
}

impl Node {
    pub fn builder(identity: Identity, trust: Arc<TrustManager>, events: EventSink) -> NodeBuilder {
        NodeBuilder {
            identity,
            trust,
            events,
            relays: vec![],
            mdns: false,
            relay_only: false,
            extra: vec![],
        }
    }

    pub fn id(&self) -> NodeId {
        self.identity.node_id()
    }

    /// Underlying endpoint (phase 07: iroh-blobs).
    pub fn endpoint(&self) -> &Endpoint {
        &self.ep
    }

    /// Connection cache for protocol clients (clip/file).
    pub fn dialer(&self) -> Dialer {
        Dialer::new(self.ep.clone(), self.gate.clone(), self.trust.clone())
    }

    /// Current relay URL + direct addresses, as sent in presence `Hello`.
    pub fn local_addrs(&self) -> Addrs {
        addrs_of(&self.ep.addr())
    }

    /// Live addresses for presence (`Hello` on every change).
    pub fn addr_feed(&self) -> AddrFeed {
        self.feed.clone()
    }

    /// Waits until a relay home connection is up, at most `max`. `false` on timeout.
    pub async fn wait_online(&self, max: Duration) -> bool {
        tokio::time::timeout(max, self.ep.online()).await.is_ok()
    }

    pub fn path_state(&self, peer: &NodeId) -> PathState {
        self.paths.state(peer)
    }

    /// Dial a paired device. Addresses come from the lookup (presence + mDNS).
    pub async fn connect(&self, peer: &NodeId, alpn: &[u8]) -> Result<Connection> {
        if !self.trust.is_trusted(peer) {
            return Err(CoreError::NotTrusted(peer.to_string()));
        }
        let conn = self.dial(peer, alpn).await?;
        self.gate.track(&conn);
        Ok(conn)
    }

    async fn dial(&self, peer: &NodeId, alpn: &[u8]) -> Result<Connection> {
        let addr = EndpointAddr::new(endpoint_id(peer)?);
        self.ep.connect(addr, alpn).await.map_err(CoreError::net)
    }

    /// New device: open a pairing window now. `OpenPairing::wait` yields the pinned list seq;
    /// dropping it closes the window.
    pub fn open_pairing(
        &self,
        secret: PairSecret,
        info: DeviceInfo,
        ttl: Duration,
    ) -> Result<OpenPairing> {
        self.windows.open_guarded(secret, info, ttl)
    }

    /// A pairing window is live or claimed (a new `open_pairing` would be refused).
    pub fn pairing_busy(&self) -> bool {
        self.windows.is_busy()
    }

    /// Closes any open pairing window (cancel).
    pub fn close_pairing(&self) {
        self.windows.close();
    }

    /// `open_pairing` + `wait`.
    pub async fn await_pairing(
        &self,
        secret: PairSecret,
        info: DeviceInfo,
        ttl: Duration,
    ) -> Result<u64> {
        self.open_pairing(secret, info, ttl)?.wait().await
    }

    /// Existing device: pair with the new device `peer`. `relay` comes from the backend lookup
    /// or QR payload. Emits `PairingEvent::ConfirmRequest`; blocks until the user decides.
    pub async fn pair_with<B: ListBackend>(
        &self,
        peer: &NodeId,
        relay: Option<&str>,
        direct: &[std::net::SocketAddr],
        secret: &PairSecret,
        backend: &B,
    ) -> Result<u64> {
        self.discovery.hint(peer, relay, direct)?;
        let conn = self.dial(peer, ALPN_PAIR).await?;
        run_initiator(
            &conn,
            &self.identity,
            secret,
            &self.trust,
            &self.events,
            backend,
        )
        .await
    }

    pub async fn shutdown(self) {
        self.close().await;
    }

    /// Graceful shutdown through a shared reference (for a node whose owner cannot be unwrapped).
    /// Idempotent: repeated calls, and a later drop, are harmless.
    pub async fn close(&self) {
        self.enforcer.abort();
        self.feeder.abort();
        let _ = self.router.shutdown().await;
        self.ep.close().await;
    }

    pub fn is_closed(&self) -> bool {
        self.ep.is_closed()
    }
}
