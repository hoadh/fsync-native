//! N8: a sender must learn that a peer which no longer trusts it refused the offer. Before the
//! app-level ack (clip/2) the transport ack made such a send look "delivered".

mod common;

use common::engine::{EngPeer, group};
use fsync_core::{
    EngineConfig,
    clip::{ClipInput, PublishOpts, PublishOutcome, Published},
};
use fsync_proto::{DeviceList, sign};
use std::time::{Duration, Instant};

async fn publish(p: &EngPeer, s: &str) -> Published {
    match p
        .engine
        .publish_clip(ClipInput::text(s), PublishOpts::default())
        .await
        .unwrap()
    {
        PublishOutcome::Sent(p) => p,
        PublishOutcome::Echo => panic!("unexpected echo"),
    }
}

/// `a` removes `b` from the account (b keeps its own, now stale, pin).
fn remove(a: &EngPeer, b: &EngPeer) {
    let head = a.trust.head().unwrap();
    let rm = DeviceList::next_remove(&head, a.nid(), &b.nid(), 3).unwrap();
    a.trust
        .ingest(&sign(a.id.signing_key(), &rm).unwrap())
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn removed_sender_is_told_refused_on_every_try() {
    let ps = group(vec![EngineConfig::default(); 2]).await;
    let (a, b) = (&ps[0], &ps[1]);
    // Warm: b holds a live connection to a, as in a long session.
    let warm = publish(b, "warm").await;
    assert_eq!(warm.delivered, vec![a.nid()]);
    remove(a, b);
    for i in 0..5 {
        let p = publish(b, &format!("after removal {i}")).await;
        assert!(p.delivered.is_empty(), "try {i}: reported delivered: {p:?}");
        assert_eq!(p.refused, vec![a.nid()], "try {i}: {p:?}");
        assert!(
            p.failed.is_empty(),
            "try {i}: refusal is not offline: {p:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn removed_sender_with_no_prior_connection_is_refused() {
    let ps = group(vec![EngineConfig::default(); 2]).await;
    let (a, b) = (&ps[0], &ps[1]);
    remove(a, b);
    let p = publish(b, "cold").await;
    assert!(p.delivered.is_empty(), "reported delivered: {p:?}");
    assert_eq!(p.refused, vec![a.nid()], "{p:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn old_clip_alpn_peer_fails_fast_not_hangs() {
    let ps = group(vec![EngineConfig::default(); 2]).await;
    let started = Instant::now();
    let r = tokio::time::timeout(
        Duration::from_secs(10),
        ps[0].node().connect(&ps[1].nid(), b"fsync/clip/1"),
    )
    .await
    .expect("must not hang");
    assert!(r.is_err(), "a clip/1 peer must not be able to talk clip");
    assert!(started.elapsed() < Duration::from_secs(10));
}
