//! Accept gate: only devices in the pinned, verified list may talk to us.
//! Exception: `fsync/pair/1` while a pairing window is open.

use super::{convert::node_id_of, path_state::PathTracker};
use crate::{pair::PairingWindows, trust::TrustManager};
use fsync_proto::{ALPN_PAIR, NodeId};
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};

/// Application close code for rejected / removed peers.
pub const CLOSE_UNTRUSTED: u32 = 403;
/// Application close code for connections dropped locally because the peer looks gone.
pub const CLOSE_STALE: u32 = 410;

/// (opened at, we dialed it, the connection)
type Live = (Instant, bool, Connection);

pub struct Gate {
    pub(crate) trust: Arc<TrustManager>,
    pub(crate) windows: Arc<PairingWindows>,
    pub(crate) paths: Arc<PathTracker>,
    live: Mutex<HashMap<(NodeId, usize), Live>>,
}

impl std::fmt::Debug for Gate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Gate")
    }
}

impl Gate {
    pub fn new(
        trust: Arc<TrustManager>,
        windows: Arc<PairingWindows>,
        paths: Arc<PathTracker>,
    ) -> Arc<Self> {
        Arc::new(Self {
            trust,
            windows,
            paths,
            live: Mutex::default(),
        })
    }

    /// The whole policy in one place.
    pub fn decide(&self, remote: &NodeId, alpn: &[u8]) -> bool {
        self.trust.is_trusted(remote) || (alpn == ALPN_PAIR && self.windows.is_open())
    }

    /// Track a trusted connection (inbound, or a one-off dial): path state + closable on device removal.
    pub fn track(self: &Arc<Self>, conn: &Connection) {
        self.track_dir(conn, false);
    }

    /// Track a connection the shared `Dialer` opened (the only kind `close_stale` may close).
    pub fn track_outbound(self: &Arc<Self>, conn: &Connection) {
        self.track_dir(conn, true);
    }

    fn track_dir(self: &Arc<Self>, conn: &Connection, outbound: bool) {
        let key = (node_id_of(conn), conn.stable_id());
        if let Ok(mut g) = self.live.lock() {
            g.insert(key, (Instant::now(), outbound, conn.clone()));
        }
        self.paths.watch(conn.clone());
        let (me, conn) = (Arc::clone(self), conn.clone());
        tokio::spawn(async move {
            conn.closed().await;
            if let Ok(mut g) = me.live.lock() {
                g.remove(&key);
            }
        });
    }

    /// Close the outbound `alpn` connections to `peer` opened before `before`. Used when a fresh
    /// dial for that same ALPN stalls: connections that predate it are presumed dead (peer
    /// crashed/moved) and their stale path state would keep blocking new handshakes. Other
    /// ALPNs and newer connections (possibly mid-transfer) are left alone.
    pub fn close_stale(&self, peer: &NodeId, alpn: &[u8], before: Instant) {
        let Ok(g) = self.live.lock() else { return };
        for ((p, _), (opened, outbound, conn)) in g.iter() {
            if *outbound && p == peer && *opened < before && conn.alpn() == alpn {
                conn.close(CLOSE_STALE.into(), b"stale");
            }
        }
    }

    /// Close live connections of devices no longer in the pinned list.
    pub fn enforce(&self) {
        let Ok(g) = self.live.lock() else { return };
        for ((peer, _), (_, _, conn)) in g.iter() {
            if !self.trust.is_trusted(peer) {
                conn.close(CLOSE_UNTRUSTED.into(), b"removed");
            }
        }
    }
}

/// Wraps any protocol handler with the accept gate.
#[derive(Debug)]
pub struct Gated<H> {
    gate: Arc<Gate>,
    inner: H,
}

impl<H> Gated<H> {
    pub fn new(gate: Arc<Gate>, inner: H) -> Self {
        Self { gate, inner }
    }
}

impl<H: ProtocolHandler> ProtocolHandler for Gated<H> {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let remote = node_id_of(&conn);
        if !self.gate.decide(&remote, conn.alpn()) {
            conn.close(CLOSE_UNTRUSTED.into(), b"untrusted");
            return Ok(());
        }
        if self.gate.trust.is_trusted(&remote) {
            self.gate.track(&conn);
        }
        self.inner.accept(conn).await
    }

    async fn shutdown(&self) {
        self.inner.shutdown().await
    }
}
