//! Engine-level harness: N paired devices of one account, each with an Engine + FsBlobStore.
#![allow(dead_code)]

use super::wait_on;
use fsync_core::{
    Engine, EngineConfig, EngineEvent, EventSink, Identity,
    blobs::{BlobStore, EncryptedBlobStore, FsBlobStore},
    history::History,
    net::Node,
    trust::{MemoryTrustStore, TrustManager},
};
use fsync_proto::{DeviceList, NodeId, Platform, sign};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::sync::mpsc;
use uuid::Uuid;

pub struct EngPeer {
    pub id: Identity,
    pub trust: Arc<TrustManager>,
    pub engine: Arc<Engine>,
    pub events: mpsc::Receiver<EngineEvent>,
    pub dir: tempfile::TempDir,
    pub history: Option<History>,
}

impl EngPeer {
    pub fn nid(&self) -> NodeId {
        self.id.node_id()
    }
    pub fn blobs_dir(&self) -> PathBuf {
        self.dir.path().join("blobs")
    }
    pub fn node(&self) -> &Node {
        self.engine.node()
    }
    pub fn loopback(&self) -> SocketAddr {
        let port = self
            .node()
            .endpoint()
            .bound_sockets()
            .iter()
            .find(|s| s.is_ipv4())
            .map(|s| s.port())
            .unwrap();
        SocketAddr::from(([127, 0, 0, 1], port))
    }
    pub async fn wait<T>(&mut self, f: impl Fn(&EngineEvent) -> Option<T>) -> T {
        wait_on(&mut self.events, f).await
    }
}

/// Unpaired engine with its own trust state.
pub async fn eng_peer(cfg: EngineConfig) -> EngPeer {
    eng_peer_with(cfg, false).await
}

/// `with_history`: encrypted blob store + SQLCipher history under the peer's temp dir.
pub async fn eng_peer_with(cfg: EngineConfig, with_history: bool) -> EngPeer {
    let id = Identity::generate().unwrap();
    let (sink, events) = EventSink::channel();
    let trust =
        Arc::new(TrustManager::new(Arc::new(MemoryTrustStore::default()), sink.clone()).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let fs = Arc::new(FsBlobStore::open(dir.path().join("blobs")).unwrap());
    let b = Node::builder(id.clone(), trust.clone(), sink.clone());
    let (engine, history) = if with_history {
        let store: Arc<dyn BlobStore> = Arc::new(EncryptedBlobStore::new(fs, &[5; 32]).unwrap());
        let h = History::open(&dir.path().join("data"), &[5; 32], store.clone(), &sink).unwrap();
        let e = Engine::start_with_history(b, store, cfg, Some(h.clone())).await;
        (e.unwrap(), Some(h))
    } else {
        (Engine::start(b, fs, cfg).await.unwrap(), None)
    };
    let engine = Arc::new(engine);
    EngPeer {
        history,
        id,
        trust,
        engine,
        events,
        dir,
    }
}

/// `n` devices of one account, all trusting each other and knowing each other's loopback address.
pub async fn group(cfgs: Vec<EngineConfig>) -> Vec<EngPeer> {
    group_with(cfgs, false).await
}

pub async fn group_with(cfgs: Vec<EngineConfig>, with_history: bool) -> Vec<EngPeer> {
    let mut ps = vec![];
    for c in cfgs {
        ps.push(eng_peer_with(c, with_history).await);
    }
    let account = Uuid::new_v4();
    let first = ps[0].id.clone();
    let mut signed = sign(
        first.signing_key(),
        &DeviceList::genesis(account, ps[0].nid(), "d0".into(), Platform::Cli, 1),
    )
    .unwrap();
    ps[0].trust.pin_genesis(&signed).unwrap();
    let mut chain = vec![signed.clone()];
    for i in 1..ps.len() {
        let l = DeviceList::next_add(
            &signed,
            ps[0].nid(),
            ps[i].nid(),
            format!("d{i}"),
            Platform::Cli,
            2,
        )
        .unwrap();
        signed = sign(first.signing_key(), &l).unwrap();
        chain.push(signed.clone());
    }
    ps[0].trust.ingest_chain(&chain).unwrap();
    for i in 1..ps.len() {
        ps[i]
            .trust
            .pin_paired_head(&signed, &ps[i].nid(), &ps[0].nid(), Some(account))
            .unwrap();
    }
    for a in &ps {
        for b in &ps {
            if a.nid() != b.nid() {
                a.node()
                    .discovery
                    .hint(&b.nid(), None, &[b.loopback()])
                    .unwrap();
            }
        }
    }
    ps
}

pub fn pseudo_random(len: usize, seed: u64) -> Vec<u8> {
    let mut s = seed | 1;
    let mut v = Vec::with_capacity(len + 8);
    while v.len() < len {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        v.extend_from_slice(&s.to_le_bytes());
    }
    v.truncate(len);
    v
}
