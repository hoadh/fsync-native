//! Several relays in one map: iroh picks the home relay by latency and ignores one that is
//! gone, so two paired peers still connect (through the live relay) when the other is down.
//! Real in-process relay servers on loopback, relay-only endpoints (no direct path possible).

mod common;
use common::{Peer, TEST_ALPN, peer_with_relays};
use fsync_core::PathState;
use iroh_relay::server::{RelayConfig, Server, ServerConfig};
use std::time::Duration;
use uuid::Uuid;

async fn relay() -> Server {
    let mut cfg = ServerConfig::default();
    cfg.relay = Some(RelayConfig::new(([127, 0, 0, 1], 0)));
    Server::spawn(cfg).await.expect("relay starts")
}

fn url_of(s: &Server) -> String {
    format!("http://{}", s.http_addr().expect("relay http addr"))
}

async fn echo(conn: &iroh::endpoint::Connection, msg: &[u8]) -> Vec<u8> {
    let (mut tx, mut rx) = conn.open_bi().await.unwrap();
    tx.write_all(msg).await.unwrap();
    tx.finish().unwrap();
    rx.read_to_end(1 << 16).await.unwrap()
}

/// Two paired relay-only peers that both use `relays`; each learns the other's relay as it
/// reports it (in the product that is the presence `Hello`).
async fn paired(relays: &[String]) -> (Peer, Peer, String) {
    let (a, b) = (
        peer_with_relays(relays.to_vec(), true).await,
        peer_with_relays(relays.to_vec(), true).await,
    );
    let g = a.genesis(Uuid::new_v4());
    let l1 = fsync_proto::DeviceList::next_add(
        &g,
        a.nid(),
        b.nid(),
        "b".into(),
        fsync_proto::Platform::Mac,
        2,
    )
    .unwrap();
    let l1 = fsync_proto::sign(a.id.signing_key(), &l1).unwrap();
    a.trust.ingest(&l1).unwrap();
    b.trust
        .pin_paired_chain(&[g, l1], &b.nid(), &a.nid())
        .unwrap();
    for p in [&a, &b] {
        assert!(
            p.node.wait_online(Duration::from_secs(15)).await,
            "no home relay"
        );
    }
    let b_home = b
        .node
        .local_addrs()
        .relay_url
        .expect("B reports a home relay");
    a.node.discovery.hint(&b.nid(), Some(&b_home), &[]).unwrap();
    b.node
        .discovery
        .hint(&a.nid(), a.node.local_addrs().relay_url.as_deref(), &[])
        .unwrap();
    (a, b, b_home)
}

async fn connect_and_echo(a: &Peer, b: &Peer) {
    let conn = tokio::time::timeout(Duration::from_secs(20), a.node.connect(&b.nid(), TEST_ALPN))
        .await
        .expect("connect timed out")
        .expect("connect");
    assert_eq!(echo(&conn, b"two relays").await, b"two relays");
    assert_eq!(a.node.path_state(&b.nid()), PathState::Relay);
}

#[tokio::test(flavor = "multi_thread")]
async fn peers_connect_when_only_one_of_two_relays_is_reachable() {
    let (live, dead) = (relay().await, relay().await);
    let (live_url, dead_url) = (url_of(&live), url_of(&dead));
    // A real relay that goes away: its URL stays configured, nothing answers there.
    dead.shutdown().await.unwrap();
    // The dead one first, so success cannot come from list order.
    let (a, b, b_home) = paired(&[dead_url, live_url.clone()]).await;
    assert_eq!(
        b_home,
        format!("{live_url}/"),
        "home relay must be the live one"
    );
    connect_and_echo(&a, &b).await;
    a.node.shutdown().await;
    b.node.shutdown().await;
    live.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn peers_connect_with_both_relays_up() {
    let (r1, r2) = (relay().await, relay().await);
    let urls = [url_of(&r1), url_of(&r2)];
    let (a, b, b_home) = paired(&urls).await;
    assert!(
        urls.iter().any(|u| format!("{u}/") == b_home),
        "home relay {b_home} is one of the configured relays"
    );
    connect_and_echo(&a, &b).await;
    a.node.shutdown().await;
    b.node.shutdown().await;
    r1.shutdown().await.unwrap();
    r2.shutdown().await.unwrap();
}
