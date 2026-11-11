//! A peer whose addresses change while a dial to it is running (it came online or restarted
//! and presence delivers the new ones mid-dial) must be reached as soon as the addresses
//! arrive, not after the dial stage timed out.

mod common;
use common::{Echo, TEST_ALPN};
use fsync_core::{
    EventSink, Identity,
    net::Node,
    trust::{MemoryTrustStore, TrustManager},
};
use fsync_proto::{DeviceList, Platform, SignedDeviceList, sign};
use std::{net::SocketAddr, sync::Arc, time::Duration, time::Instant};
use uuid::Uuid;

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

async fn spawn(id: &Identity, chain: &[SignedDeviceList], me_is_b: bool) -> Node {
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
    node
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dial_follows_addresses_that_change_mid_dial() {
    let (ida, idb) = (Identity::generate().unwrap(), Identity::generate().unwrap());
    let chain = chain_of(&ida, &idb);
    let a = spawn(&ida, &chain, false).await;
    let b = spawn(&idb, &chain, true).await;
    // B was known at an old address (it restarted on a new port): the dial hangs on it.
    let stale = SocketAddr::from(([127, 0, 0, 1], 9));
    a.discovery.hint(&idb.node_id(), None, &[stale]).unwrap();
    let dialer = a.dialer();
    let b_id = idb.node_id();
    let started = Instant::now();
    let dial = tokio::spawn(async move { dialer.connect(&b_id, TEST_ALPN).await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Presence delivers B's current address while the dial is running.
    a.discovery
        .hint(&idb.node_id(), None, &[loopback(&b)])
        .unwrap();
    let conn = tokio::time::timeout(Duration::from_secs(20), dial)
        .await
        .expect("dial finished")
        .unwrap()
        .expect("dial ok");
    let took = started.elapsed();
    drop(conn);
    // The dial stage is 2 s: a dial that only re-reads the addresses at the stage end needs more.
    assert!(
        took < Duration::from_millis(1500),
        "dial took {took:?}: it did not follow the new address"
    );
}
