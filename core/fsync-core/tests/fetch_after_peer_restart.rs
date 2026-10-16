//! A peer that crashes and restarts (same node id, new port) must not make the receiver's next
//! fetch hang on the dead cached `file/1` connection until the 30 s stall timeout: the receiver
//! must drop that connection, redial once and succeed within seconds.

mod common;

use common::{
    engine::{EngPeer, eng_peer},
    files::{hash_file, write_src},
};
use fsync_core::{
    Engine, EngineConfig, EngineEvent, EventSink, Identity,
    blobs::{BlobStore, FsBlobStore},
    clip::{ClipInput, FileInput, PublishOpts},
    net::Node,
    trust::TrustManager,
};
use fsync_proto::{DeviceList, FileMeta, Platform, sign};
use std::{net::SocketAddr, path::Path, sync::Arc, time::Duration, time::Instant};
use tokio::{runtime::Runtime, sync::mpsc};
use uuid::Uuid;

/// Start a fresh engine for an existing identity, as a process restart would.
async fn restart(
    id: &Identity,
    trust: &Arc<TrustManager>,
    dir: &Path,
) -> (Engine, mpsc::Receiver<EngineEvent>) {
    let (sink, events) = EventSink::channel();
    let fs: Arc<dyn BlobStore> = Arc::new(FsBlobStore::open(dir.join("blobs")).unwrap());
    let b = Node::builder(id.clone(), trust.clone(), sink);
    let e = Engine::start(b, fs, EngineConfig::default()).await.unwrap();
    (e, events)
}

fn loopback(e: &Engine) -> SocketAddr {
    let port = e
        .node()
        .endpoint()
        .bound_sockets()
        .iter()
        .find(|s| s.is_ipv4())
        .map(|s| s.port())
        .unwrap();
    SocketAddr::from(([127, 0, 0, 1], port))
}

/// `from` offers `path`; B sees the offer.
async fn offer(from: &Engine, b: &mut EngPeer, path: &Path) -> (Uuid, FileMeta) {
    from.publish_clip(
        ClipInput::files(vec![FileInput::new(path)]),
        PublishOpts::default(),
    )
    .await
    .unwrap();
    b.wait(|e| match e {
        EngineEvent::RemoteClip { offer, .. } => Some((offer.item_id, offer.files[0].clone())),
        _ => None,
    })
    .await
}

#[test]
fn second_fetch_after_sender_crash_recovers_within_seconds() {
    let (rt_a1, rt_a2, rt_b) = (
        Runtime::new().unwrap(),
        Runtime::new().unwrap(),
        Runtime::new().unwrap(),
    );
    let a = rt_a1.block_on(eng_peer(EngineConfig::default()));
    let mut b = rt_b.block_on(eng_peer(EngineConfig::default()));
    // A owns the account, B is its second device.
    let g = sign(
        a.id.signing_key(),
        &DeviceList::genesis(Uuid::new_v4(), a.nid(), "a".into(), Platform::Cli, 1),
    )
    .unwrap();
    let l1 = DeviceList::next_add(&g, a.nid(), b.nid(), "b".into(), Platform::Cli, 2).unwrap();
    let chain = [g, sign(a.id.signing_key(), &l1).unwrap()];
    a.trust.pin_genesis(&chain[0]).unwrap();
    a.trust.ingest(&chain[1]).unwrap();
    b.trust
        .pin_paired_chain(&chain, &b.nid(), &a.nid())
        .unwrap();
    a.node()
        .discovery
        .hint(&b.nid(), None, &[b.loopback()])
        .unwrap();
    b.node()
        .discovery
        .hint(&a.nid(), None, &[a.loopback()])
        .unwrap();

    let src = write_src(a.dir.path(), "f.bin", 3 << 20, 7);
    // Different content: B must really go to the network for the second fetch.
    let src2 = write_src(a.dir.path(), "g.bin", 3 << 20, 8);
    let want = hash_file(&src2);

    // First fetch warms B's cached file/1 connection to A.
    rt_b.block_on(async {
        let (id, f) = offer(&a.engine, &mut b, &src).await;
        b.engine.fetch_file(id, f.file_id).await.unwrap();
    });

    // Crash: no CONNECTION_CLOSE reaches B.
    let (a_id, a_trust, a_dir) = (a.id.clone(), a.trust.clone(), a.dir);
    drop(a.engine);
    rt_a1.shutdown_background();

    let (a2, _a2_events) = rt_a2.block_on(restart(&a_id, &a_trust, a_dir.path()));
    a2.node()
        .discovery
        .hint(&b.nid(), None, &[b.loopback()])
        .unwrap();
    b.node()
        .discovery
        .hint(&a_id.node_id(), None, &[loopback(&a2)])
        .unwrap();
    rt_b.block_on(async {
        let (id, f) = offer(&a2, &mut b, &src2).await;
        let t = Instant::now();
        let dest = b.dir.path().join("out");
        std::fs::create_dir(&dest).unwrap();
        let got = tokio::time::timeout(
            Duration::from_secs(25),
            b.engine.fetch_file_to(id, f.file_id, &dest),
        )
        .await
        .expect("fetch hung on the dead cached connection")
        .expect("fetch");
        let took = t.elapsed();
        assert!(took < Duration::from_secs(10), "second fetch took {took:?}");
        assert_eq!(hash_file(&got), want);
    });
}
