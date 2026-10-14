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

const OTHER_ALPN: &[u8] = b"fsync/test-other/1";

struct Side {
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
        .protocol(OTHER_ALPN, Echo)
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
    Side { node }
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

fn chain_of(ida: &Identity, idb: &Identity) -> [SignedDeviceList; 2] {
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
    [g, sign(ida.signing_key(), &l1).unwrap()]
}

/// Recovery after a stalled dial closes only older connections of the same ALPN; a live
/// connection of another ALPN (e.g. a running file transfer) is untouched.
#[test]
fn closing_stale_connections_spares_other_alpns() {
    let (ida, idb) = (Identity::generate().unwrap(), Identity::generate().unwrap());
    let chain = chain_of(&ida, &idb);
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let a = spawn(&ida, &chain, false).await;
        let b = spawn(&idb, &chain, true).await;
        a.node
            .discovery
            .hint(&idb.node_id(), None, &[loopback(&b.node)])
            .unwrap();
        let d = a.node.dialer();
        let stale = d.connect(&idb.node_id(), TEST_ALPN).await.unwrap();
        let other = d.connect(&idb.node_id(), OTHER_ALPN).await.unwrap();
        a.node
            .gate()
            .close_stale(&idb.node_id(), TEST_ALPN, Instant::now());
        tokio::time::timeout(Duration::from_secs(5), stale.closed())
            .await
            .expect("stale closed");
        assert!(other.close_reason().is_none(), "other ALPN must stay open");
        let (mut tx, mut rx) = other.open_bi().await.unwrap();
        tx.write_all(b"ok").await.unwrap();
        tx.finish().unwrap();
        assert_eq!(rx.read_to_end(8).await.unwrap(), b"ok");
        // A connection opened after the cutoff is not stale either.
        let fresh = d.connect(&idb.node_id(), TEST_ALPN).await.unwrap();
        a.node.gate().close_stale(
            &idb.node_id(),
            TEST_ALPN,
            Instant::now() - Duration::from_secs(60),
        );
        assert!(
            fresh.close_reason().is_none(),
            "newer connection must survive"
        );
    });
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
}
