//! `Node::peer_path_tier` on real iroh endpoints: loopback peers report `Lan`, relay-only peers
//! (real in-process relay server) report `Relay`, a peer with no connection reports `None`.

mod common;
use common::{Peer, TEST_ALPN, peer, peer_with_relays};
use fsync_core::PathTier;
use fsync_proto::{DeviceList, Platform, sign};
use iroh_relay::server::{RelayConfig, Server, ServerConfig};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// A owns the account, B is its second device.
fn pair(a: &Peer, b: &Peer) {
    let g = a.genesis(Uuid::new_v4());
    let l1 = DeviceList::next_add(&g, a.nid(), b.nid(), "b".into(), Platform::Mac, 2).unwrap();
    let l1 = sign(a.id.signing_key(), &l1).unwrap();
    a.trust.ingest(&l1).unwrap();
    b.trust
        .pin_paired_chain(&[g, l1], &b.nid(), &a.nid())
        .unwrap();
}

async fn echo(conn: &iroh::endpoint::Connection) {
    let (mut tx, mut rx) = conn.open_bi().await.unwrap();
    tx.write_all(b"tier").await.unwrap();
    tx.finish().unwrap();
    assert_eq!(rx.read_to_end(1 << 16).await.unwrap(), b"tier");
}

/// The tier of the selected path settles shortly after the first data; poll instead of sleeping.
async fn tier_of(from: &Peer, to: &Peer) -> Option<PathTier> {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let t = from.node.peer_path_tier(&to.nid());
        if t.is_some() || Instant::now() > end {
            return t;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn loopback_peers_report_lan_and_none_without_connection() {
    let (a, b) = (peer().await, peer().await);
    pair(&a, &b);
    a.learn(&b);
    b.learn(&a);
    assert_eq!(b.node.peer_path_tier(&a.nid()), None, "no connection yet");

    let conn = b.node.connect(&a.nid(), TEST_ALPN).await.unwrap();
    echo(&conn).await;
    assert_eq!(tier_of(&b, &a).await, Some(PathTier::Lan));
    // The accepting side tracks the inbound connection too.
    assert_eq!(tier_of(&a, &b).await, Some(PathTier::Lan));

    drop(conn);
    let stranger = peer().await;
    assert_eq!(b.node.peer_path_tier(&stranger.nid()), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn relay_only_peers_report_relay() {
    let mut cfg = ServerConfig::default();
    cfg.relay = Some(RelayConfig::new(([127, 0, 0, 1], 0)));
    let relay = Server::spawn(cfg).await.expect("relay starts");
    let url = format!("http://{}", relay.http_addr().expect("relay http addr"));

    let (a, b) = (
        peer_with_relays(vec![url.clone()], true).await,
        peer_with_relays(vec![url.clone()], true).await,
    );
    pair(&a, &b);
    for p in [&a, &b] {
        assert!(
            p.node.wait_online(Duration::from_secs(15)).await,
            "no home relay"
        );
    }
    a.node
        .discovery
        .hint(&b.nid(), b.node.local_addrs().relay_url.as_deref(), &[])
        .unwrap();
    b.node
        .discovery
        .hint(&a.nid(), a.node.local_addrs().relay_url.as_deref(), &[])
        .unwrap();

    let conn = tokio::time::timeout(Duration::from_secs(20), a.node.connect(&b.nid(), TEST_ALPN))
        .await
        .expect("connect timed out")
        .expect("connect");
    echo(&conn).await;
    assert_eq!(tier_of(&a, &b).await, Some(PathTier::Relay));
}
