//! An offline account device must not slow down publishing to the others: the first clip may pay
//! for finding out, every following one must not be held up by the dead peer.

mod common;

use common::engine::{EngPeer, group};
use fsync_core::{
    EngineConfig, EngineEvent,
    clip::{ClipInput, PublishOpts, PublishOutcome, Published},
};
use std::time::{Duration, Instant};

async fn publish(p: &EngPeer, s: &str) -> (Published, Duration) {
    let t = Instant::now();
    let o = p
        .engine
        .publish_clip(ClipInput::text(s), PublishOpts::default())
        .await
        .unwrap();
    match o {
        PublishOutcome::Sent(p) => (p, t.elapsed()),
        PublishOutcome::Echo => panic!("unexpected echo"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn offline_peer_does_not_delay_following_publishes() {
    let mut ps = group(vec![EngineConfig::default(); 4]).await;
    let dead = ps.pop().unwrap();
    dead.engine.close().await;
    let (sender, rest) = ps.split_first_mut().unwrap();
    let live: Vec<_> = rest.iter().map(|p| p.nid()).collect();

    // The first publish finds out the peer is gone (full cost is allowed here).
    let (first, _) = publish(sender, "first").await;
    assert_eq!(first.failed, vec![dead.nid()], "{first:?}");

    for i in 0..3 {
        let (p, took) = publish(sender, &format!("clip {i}")).await;
        assert_eq!(p.delivered.len(), live.len(), "clip {i}: {p:?}");
        assert!(p.failed.contains(&dead.nid()), "clip {i}: {p:?}");
        assert!(
            took < Duration::from_secs(1),
            "clip {i}: publish held up {took:?} by the offline peer"
        );
    }
    for r in rest.iter_mut() {
        let mut seen = 0;
        while seen < 4 {
            let ev = tokio::time::timeout(Duration::from_secs(1), r.events.recv())
                .await
                .expect("reachable peer missed a clip")
                .unwrap();
            if matches!(ev, EngineEvent::RemoteClip { .. }) {
                seen += 1;
            }
        }
    }
}
