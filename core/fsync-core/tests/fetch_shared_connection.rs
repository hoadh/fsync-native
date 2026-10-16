//! The cached file/1 connection is shared by every fetch to one peer. A retry by one fetch must
//! not cut off healthy siblings, and a slow but healthy answer must not be taken for a dead path.

mod common;

use common::{
    TEST_ALPN,
    engine::group,
    files::{hash_file, offer_file, write_src},
    peer,
};
use fsync_core::EngineConfig;
use fsync_proto::{DeviceList, Platform, sign};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Dialer-level: `abandon` evicts only the connection it was given, and closes it only when it
/// received nothing since the caller started (in-flight streams of a healthy one always do).
#[tokio::test(flavor = "multi_thread")]
async fn abandon_spares_a_connection_with_traffic_and_a_newer_cache_entry() {
    let (a, b) = (peer().await, peer().await);
    let g = a.genesis(Uuid::new_v4());
    let l1 = DeviceList::next_add(&g, a.nid(), b.nid(), "b".into(), Platform::Mac, 2).unwrap();
    let l1 = sign(a.id.signing_key(), &l1).unwrap();
    a.trust.ingest(&l1).unwrap();
    b.trust
        .pin_paired_chain(&[g, l1], &b.nid(), &a.nid())
        .unwrap();
    a.learn(&b);
    b.learn(&a);

    let dialer = b.node.dialer();
    let (c1, reused) = dialer
        .connect_reporting_reuse(&a.nid(), TEST_ALPN)
        .await
        .unwrap();
    assert!(!reused);
    let rx_before = c1.stats().udp_rx.bytes;
    // A sibling stream is mid-flight on the shared connection: it writes and gets acks.
    let (mut tx, mut rx) = c1.open_bi().await.unwrap();
    tx.write_all(b"in flight").await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(c1.stats().udp_rx.bytes > rx_before, "no ack seen");

    dialer.abandon(&a.nid(), TEST_ALPN, &c1, rx_before);
    assert!(c1.close_reason().is_none(), "healthy connection was closed");
    tx.finish().unwrap();
    assert_eq!(rx.read_to_end(1 << 16).await.unwrap(), b"in flight");

    // The cache entry is gone, so the next caller dials a fresh connection ...
    let c2 = dialer.connect(&a.nid(), TEST_ALPN).await.unwrap();
    assert_ne!(c1.stable_id(), c2.stable_id());
    // ... and a late `abandon` of the old one does not evict it.
    dialer.abandon(&a.nid(), TEST_ALPN, &c1, 0);
    let c3 = dialer.connect(&a.nid(), TEST_ALPN).await.unwrap();
    assert_eq!(c2.stable_id(), c3.stable_id());

    // Silence since the mark means dead: closed. Let the fresh connection's handshake tail
    // (tokens, path probes) settle first so the mark is taken in a quiet moment.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    dialer.abandon(&a.nid(), TEST_ALPN, &c2, c2.stats().udp_rx.bytes);
    assert!(c2.close_reason().is_some(), "silent connection stayed open");
}

/// An answer that is slow (3 s) but under the 5 s deadline is waited for on the reused
/// connection: no redial, same connection afterwards.
#[tokio::test(flavor = "multi_thread")]
async fn slow_healthy_answer_on_a_cached_connection_is_not_abandoned() {
    let slow = EngineConfig {
        serve_delay: Some(Duration::from_secs(3)),
        ..EngineConfig::default()
    };
    let mut ps = group(vec![slow, EngineConfig::default()]).await;
    let (src1, src2) = (
        write_src(ps[0].dir.path(), "one.bin", 1 << 20, 11),
        write_src(ps[0].dir.path(), "two.bin", 1 << 20, 12),
    );
    let out = ps[1].dir.path().join("out");
    std::fs::create_dir(&out).unwrap();

    let (id, f) = offer_file(&mut ps, &src1).await;
    ps[1]
        .engine
        .fetch_file_to(id, f.file_id, &out)
        .await
        .unwrap();
    let before: Vec<usize> = ps[1]
        .node()
        .gate()
        .connections_of(&ps[0].nid())
        .iter()
        .map(|c| c.stable_id())
        .collect();
    assert!(!before.is_empty());

    let (id, f) = offer_file(&mut ps, &src2).await;
    let t = Instant::now();
    let got = ps[1]
        .engine
        .fetch_file_to(id, f.file_id, &out)
        .await
        .unwrap();
    assert!(t.elapsed() >= Duration::from_secs(3), "delay hook not hit");
    assert_eq!(hash_file(&got), hash_file(&src2));

    let after = ps[1].node().gate().connections_of(&ps[0].nid());
    for sid in before {
        let c = after
            .iter()
            .find(|c| c.stable_id() == sid)
            .expect("conn kept");
        assert!(c.close_reason().is_none());
    }
}
