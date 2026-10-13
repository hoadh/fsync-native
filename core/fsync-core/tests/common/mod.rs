//! Shared harness: in-process peers on loopback (no relay, no discovery services).
#![allow(dead_code)]

pub mod engine;
pub mod files;
pub mod pairing;

use fsync_core::{
    CoreError, EngineEvent, EventSink, Identity, ListBackend, Result,
    error::ApiError,
    net::Node,
    trust::{MemoryTrustStore, TrustManager},
};
use fsync_proto::{DeviceList, NodeId, Platform, SignedDeviceList, sign, verify_next};
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;
use uuid::Uuid;

pub const TEST_ALPN: &[u8] = b"fsync/test/1";

/// Echo handler: reads one bi-stream to the end and writes it back.
#[derive(Debug, Clone)]
pub struct Echo;

impl ProtocolHandler for Echo {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let (mut tx, mut rx) = conn.accept_bi().await?;
        let data = rx
            .read_to_end(1 << 16)
            .await
            .map_err(AcceptError::from_err)?;
        tx.write_all(&data).await.map_err(AcceptError::from_err)?;
        tx.finish().map_err(AcceptError::from_err)?;
        let _ = tokio::time::timeout(Duration::from_secs(2), conn.closed()).await;
        Ok(())
    }
}

pub struct Peer {
    pub id: Identity,
    pub trust: Arc<TrustManager>,
    pub node: Node,
    pub events: mpsc::Receiver<EngineEvent>,
}

pub async fn peer() -> Peer {
    let id = Identity::generate().unwrap();
    let (sink, events) = EventSink::channel();
    let trust =
        Arc::new(TrustManager::new(Arc::new(MemoryTrustStore::default()), sink.clone()).unwrap());
    let node = Node::builder(id.clone(), trust.clone(), sink)
        .protocol(TEST_ALPN, Echo)
        .spawn()
        .await
        .unwrap();
    Peer {
        id,
        trust,
        node,
        events,
    }
}

impl Peer {
    pub fn nid(&self) -> NodeId {
        self.id.node_id()
    }

    /// Loopback address of this peer's endpoint.
    pub fn loopback(&self) -> SocketAddr {
        let port = self
            .node
            .endpoint()
            .bound_sockets()
            .iter()
            .find(|s| s.is_ipv4())
            .map(|s| s.port())
            .unwrap();
        SocketAddr::from(([127, 0, 0, 1], port))
    }

    /// Tell this peer how to reach `other`.
    pub fn learn(&self, other: &Peer) {
        self.node
            .discovery
            .hint(&other.nid(), None, &[other.loopback()])
            .unwrap();
    }

    /// Genesis list pinned as the first device of `account`.
    pub fn genesis(&self, account: Uuid) -> SignedDeviceList {
        let l = DeviceList::genesis(account, self.nid(), "first".into(), Platform::Cli, 1);
        let s = sign(self.id.signing_key(), &l).unwrap();
        self.trust.pin_genesis(&s).unwrap();
        s
    }

    /// Wait for an event matching `f` (10 s).
    pub async fn wait<T>(&mut self, f: impl Fn(&EngineEvent) -> Option<T>) -> T {
        wait_on(&mut self.events, f).await
    }
}

pub async fn wait_on<T>(
    rx: &mut mpsc::Receiver<EngineEvent>,
    f: impl Fn(&EngineEvent) -> Option<T>,
) -> T {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let e = rx.recv().await.expect("events closed");
            if let Some(v) = f(&e) {
                return v;
            }
        }
    })
    .await
    .expect("timed out waiting for event")
}

/// Tiny in-memory registry standing in for the backend (verifies like the real one).
pub struct MemBackend(pub Mutex<Vec<SignedDeviceList>>);

impl MemBackend {
    pub fn with(genesis: SignedDeviceList) -> Self {
        Self(Mutex::new(vec![genesis]))
    }
}

impl ListBackend for MemBackend {
    async fn publish(&self, list: &SignedDeviceList, prev_seq: u64) -> Result<()> {
        let mut g = self.0.lock().unwrap();
        let head = g.last().unwrap().clone();
        let conflict = || {
            CoreError::Api(ApiError {
                status: 409,
                code: "conflict".into(),
                message: String::new(),
            })
        };
        if head.decode().unwrap().seq != prev_seq {
            return Err(conflict());
        }
        verify_next(list, &head).map_err(|_| conflict())?;
        g.push(list.clone());
        Ok(())
    }
    async fn sync(&self) -> Result<()> {
        Ok(())
    }
    async fn full_chain(&self) -> Result<Vec<SignedDeviceList>> {
        Ok(self.0.lock().unwrap().clone())
    }
}
