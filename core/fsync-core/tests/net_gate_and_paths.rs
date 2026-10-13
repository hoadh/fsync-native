//! Accept gate + path state on real iroh endpoints over loopback.

mod common;
use common::{Peer, TEST_ALPN, peer};
use fsync_core::{EngineEvent, PathState};
use iroh::EndpointAddr;
use std::time::Duration;
use uuid::Uuid;

async fn echo(conn: &iroh::endpoint::Connection, msg: &[u8]) -> Result<Vec<u8>, String> {
    let (mut tx, mut rx) = conn.open_bi().await.map_err(|e| e.to_string())?;
    tx.write_all(msg).await.map_err(|e| e.to_string())?;
    tx.finish().map_err(|e| e.to_string())?;
    rx.read_to_end(1 << 16).await.map_err(|e| e.to_string())
}

/// Two paired peers (A, B); both pin the same 2-device list.
async fn paired() -> (Peer, Peer) {
    let (a, b) = (peer().await, peer().await);
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
    a.learn(&b);
    b.learn(&a);
    (a, b)
}

#[tokio::test]
async fn paired_peers_talk_and_report_lan() {
    let (a, mut b) = paired().await;
    let conn = b.node.connect(&a.nid(), TEST_ALPN).await.unwrap();
    assert_eq!(echo(&conn, b"hello").await.unwrap(), b"hello");
    let state = b
        .wait(|e| match e {
            EngineEvent::DeviceStatus { node_id, state }
                if *node_id == a.nid() && *state != PathState::Offline =>
            {
                Some(*state)
            }
            _ => None,
        })
        .await;
    assert_eq!(state, PathState::Lan, "loopback must be a direct IP path");
    assert_eq!(b.node.path_state(&a.nid()), PathState::Lan);
    drop(conn);
}

#[tokio::test]
async fn unpaired_endpoint_is_rejected_by_accept_gate() {
    let (a, _b) = paired().await;
    let stranger = peer().await;
    // Bypass Node::connect (which refuses untrusted peers) and dial raw.
    let addr = EndpointAddr::new(a.node.endpoint().id()).with_ip_addr(a.loopback());
    let attempt = async {
        let conn = stranger
            .node
            .endpoint()
            .connect(addr, TEST_ALPN)
            .await
            .map_err(|e| e.to_string())?;
        echo(&conn, b"let me in").await
    };
    let res = tokio::time::timeout(Duration::from_secs(10), attempt)
        .await
        .expect("gate must close promptly");
    assert!(
        res.is_err(),
        "unpaired connection must not get an echo: {res:?}"
    );
}

#[tokio::test]
async fn unpaired_cannot_use_pair_alpn_without_open_window() {
    let (a, _b) = paired().await;
    let stranger = peer().await;
    let addr = EndpointAddr::new(a.node.endpoint().id()).with_ip_addr(a.loopback());
    let res = tokio::time::timeout(Duration::from_secs(10), async {
        let conn = stranger
            .node
            .endpoint()
            .connect(addr, fsync_proto::ALPN_PAIR)
            .await
            .map_err(|e| e.to_string())?;
        let (mut tx, mut rx) = conn.open_bi().await.map_err(|e| e.to_string())?;
        tx.write_all(b"x").await.map_err(|e| e.to_string())?;
        tx.finish().map_err(|e| e.to_string())?;
        rx.read_to_end(1024).await.map_err(|e| e.to_string())
    })
    .await
    .expect("closed promptly");
    assert!(res.is_err() || res.unwrap().is_empty());
}

#[tokio::test]
async fn connect_refuses_untrusted_target() {
    let (a, _b) = paired().await;
    let stranger = peer().await;
    assert!(matches!(
        a.node.connect(&stranger.nid(), TEST_ALPN).await,
        Err(fsync_core::CoreError::NotTrusted(_))
    ));
}

#[tokio::test]
async fn removing_a_device_closes_its_live_connection() {
    let (a, b) = paired().await;
    let conn = b.node.connect(&a.nid(), TEST_ALPN).await.unwrap();
    assert_eq!(echo(&conn, b"x").await.unwrap(), b"x");
    // A removes B: the enforcer must close B's live connections on A.
    let head = a.trust.head().unwrap();
    let rm = fsync_proto::DeviceList::next_remove(&head, a.nid(), &b.nid(), 3).unwrap();
    a.trust
        .ingest(&fsync_proto::sign(a.id.signing_key(), &rm).unwrap())
        .unwrap();
    let closed = tokio::time::timeout(Duration::from_secs(10), conn.closed()).await;
    assert!(closed.is_ok(), "connection must be closed after removal");
}
