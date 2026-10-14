//! A paired peer that restarts (same node id, new port) must be reachable again in seconds,
//! not after a QUIC idle timeout: the sender must not keep using the dead connection it cached.

mod common;
use common::{Echo, TEST_ALPN};
use fsync_core::{
    EventSink, Identity,
    net::Node,
    trust::{MemoryTrustStore, TrustManager},
};
use fsync_proto::{DeviceList, Platform, SignedDeviceList, sign};
use std::{net::SocketAddr, sync::Arc, time::Duration, time::Instant};
use tokio::runtime::Runtime;
use uuid::Uuid;

struct Side {
    id: Identity,
    node: Node,
}

fn loopback(n: &Node) -> SocketAddr {
    let port = n
        .endpoint()
        .bound_sockets()
        .iter()
        .find(|s| s.is_ipv4())
        .map(|s| s.port())
        .unwrap();
    SocketAddr::from(([127, 0, 0, 1], port))
}

async fn spawn(id: &Identity, chain: &[SignedDeviceList], me_is_b: bool) -> Side {
    let (sink, _events) = EventSink::channel();
    let trust =
        Arc::new(TrustManager::new(Arc::new(MemoryTrustStore::default()), sink.clone()).unwrap());
    let node = Node::builder(id.clone(), trust.clone(), sink)
        .protocol(TEST_ALPN, Echo)
        .spawn()
        .await
        .unwrap();
    if me_is_b {
        let a = chain[0].decode().unwrap().devices[0].node_id;
        trust.pin_paired_chain(chain, &id.node_id(), &a).unwrap();
    } else {
        trust.pin_genesis(&chain[0]).unwrap();
        trust.ingest(&chain[1]).unwrap();
    }
    Side {
        id: id.clone(),
        node,
    }
}

async fn echo_once(a: &Side, b: &Identity) -> Duration {
    let t = Instant::now();
    let d = a.node.dialer();
    let conn = tokio::time::timeout(Duration::from_secs(60), d.connect(&b.node_id(), TEST_ALPN))
        .await
        .expect("dial timed out")
        .expect("dial");
    let (mut tx, mut rx) = conn.open_bi().await.unwrap();
    tx.write_all(b"hi").await.unwrap();
    tx.finish().unwrap();
    let got = tokio::time::timeout(Duration::from_secs(60), rx.read_to_end(64))
        .await
        .expect("echo timed out")
        .unwrap();
    assert_eq!(got, b"hi");
    t.elapsed()
}

#[test]
fn crashed_peer_is_reachable_again_quickly() {
    run(false);
}

#[test]
fn gracefully_restarted_peer_is_reachable_again_quickly() {
    run(true);
}

fn run(graceful: bool) {
    let (ida, idb) = (Identity::generate().unwrap(), Identity::generate().unwrap());
    let g = sign(
        ida.signing_key(),
        &DeviceList::genesis(Uuid::new_v4(), ida.node_id(), "a".into(), Platform::Cli, 1),
    )
    .unwrap();
    let l1 = DeviceList::next_add(
        &g,
        ida.node_id(),
        idb.node_id(),
        "b".into(),
        Platform::Cli,
        2,
    )
    .unwrap();
    let chain = [g, sign(ida.signing_key(), &l1).unwrap()];

    let rt_a = Runtime::new().unwrap();
    let rt_b1 = Runtime::new().unwrap();
    let a = rt_a.block_on(spawn(&ida, &chain, false));
    let b1 = rt_b1.block_on(spawn(&idb, &chain, true));
    a.node
        .discovery
        .hint(&idb.node_id(), None, &[loopback(&b1.node)])
        .unwrap();
    rt_a.block_on(async {
        let t = echo_once(&a, &idb).await;
        assert!(t < Duration::from_secs(5), "first dial took {t:?}");
    });
    // Crash: no CONNECTION_CLOSE reaches A. Graceful: the node closes first.
    if graceful {
        rt_b1.block_on(b1.node.close());
    }
    drop(b1);
    rt_b1.shutdown_background();

    let rt_b2 = Runtime::new().unwrap();
    let b2 = rt_b2.block_on(spawn(&idb, &chain, true));
    a.node
        .discovery
        .hint(&idb.node_id(), None, &[loopback(&b2.node)])
        .unwrap();
    let t = rt_a.block_on(echo_once(&a, &idb));
    assert!(t < Duration::from_secs(5), "took {t:?}");
    let _ = (&a.id, &b2.id);
}
