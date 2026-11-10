//! An offline account device must not slow down publishing to the others: `publish` returns after
//! `publish_wait` and the dead peer is tried in the background. A device that comes back gets the
//! next clip, an explicit push still waits for the real result, and closing the engine with a
//! send still running neither hangs nor panics.

mod common;

use common::engine::{EngPeer, group};
use fsync_core::{
    Engine, EngineConfig, EngineEvent, EventSink,
    blobs::FsBlobStore,
    clip::{ClipInput, PublishOpts, PublishOutcome, Published},
    net::Node,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;

/// Well above the default `publish_wait` (1.5 s), well below a dead peer's cost (7 s).
const QUICK: Duration = Duration::from_secs(2);

async fn publish_with(p: &EngPeer, s: &str, opts: PublishOpts) -> (Published, Duration) {
    let t = Instant::now();
    let o = p
        .engine
        .publish_clip(ClipInput::text(s), opts)
        .await
        .unwrap();
    match o {
        PublishOutcome::Sent(p) => (p, t.elapsed()),
        PublishOutcome::Echo => panic!("unexpected echo"),
    }
}

async fn publish(p: &EngPeer, s: &str) -> (Published, Duration) {
    publish_with(p, s, PublishOpts::default()).await
}

/// Item ids of the next `n` clips `p` receives, in arrival order.
async fn received(p: &mut EngPeer, n: usize, within: Duration) -> Vec<Uuid> {
    let mut ids = vec![];
    tokio::time::timeout(within, async {
        while ids.len() < n {
            if let EngineEvent::RemoteClip { offer, .. } = p.events.recv().await.unwrap() {
                ids.push(offer.item_id);
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("got {} of {n} clips: {ids:?}", ids.len()));
    ids
}

#[tokio::test(flavor = "multi_thread")]
async fn offline_peer_never_holds_up_a_publish() {
    let mut ps = group(vec![EngineConfig::default(); 4]).await;
    let dead = ps.pop().unwrap();
    dead.engine.close().await;
    let (sender, rest) = ps.split_first_mut().unwrap();
    let mut live: Vec<_> = rest.iter().map(|p| p.nid()).collect();
    live.sort();

    let mut sent = vec![];
    // Every publish, the first one included: the dead peer is left to the background.
    for i in 0..4 {
        let (mut p, took) = publish(sender, &format!("clip {i}")).await;
        assert!(took < QUICK, "clip {i}: publish held up {took:?}: {p:?}");
        p.delivered.sort();
        assert_eq!(p.delivered, live, "clip {i}: {p:?}");
        assert_eq!(p.pending, vec![dead.nid()], "clip {i}: {p:?}");
        // The first clip's attempt (7 s) still runs: later clips do not wait for the dead peer.
        if i > 0 {
            assert!(took < Duration::from_secs(1), "clip {i}: waited {took:?}");
        }
        sent.push(p.item_id);
    }
    for r in rest.iter_mut() {
        assert_eq!(received(r, 4, Duration::from_secs(5)).await, sent);
    }
}

/// A new engine with `old`'s identity and trust (the device restarted), reachable from `by`.
async fn restart(old: &EngPeer, by: &EngPeer) -> EngPeer {
    let (sink, events) = EventSink::channel();
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(FsBlobStore::open(dir.path().join("blobs")).unwrap());
    let b = Node::builder(old.id.clone(), old.trust.clone(), sink);
    let engine = Engine::start(b, store, EngineConfig::default())
        .await
        .unwrap();
    let back = EngPeer {
        id: old.id.clone(),
        trust: old.trust.clone(),
        engine: Arc::new(engine),
        events,
        dir,
        history: None,
    };
    by.node()
        .discovery
        .hint(&back.nid(), None, &[back.loopback()])
        .unwrap();
    back
}

#[tokio::test(flavor = "multi_thread")]
async fn returning_peer_gets_the_next_clip() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    let (a, b) = (&ps[0], &ps[1]);
    let (warm, _) = publish(a, "warm").await;
    assert_eq!(warm.delivered, vec![b.nid()]);
    b.engine.close().await;

    let (away, took) = publish(a, "while away").await;
    assert!(took < QUICK, "held up {took:?}");
    assert!(away.delivered.is_empty(), "{away:?}");
    let mut back = restart(&ps[1], &ps[0]).await;
    let a = &mut ps[0];

    let (next, took) = publish(a, "back").await;
    assert!(took < QUICK, "held up {took:?}");
    assert!(
        next.failed.is_empty(),
        "nothing marks the returning peer: {next:?}"
    );
    // It may still be waiting behind the "while away" attempt (up to 7 s), so allow for that.
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut got = vec![];
    while !got.contains(&next.item_id) {
        let left = deadline.saturating_duration_since(Instant::now());
        got.extend(received(&mut back, 1, left).await);
    }
    // The older clip, if its retry reached the restarted device, came first.
    assert_eq!(got.last(), Some(&next.item_id), "{got:?}");
    assert!(
        got.iter()
            .all(|id| *id == away.item_id || *id == next.item_id)
    );
    back.engine.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn push_to_offline_peer_waits_for_the_real_result() {
    let cfg = EngineConfig {
        send_timeout: Duration::from_secs(1),
        publish_wait: Duration::from_millis(200),
        ..EngineConfig::default()
    };
    let ps = group(vec![cfg; 2]).await;
    let (a, b) = (&ps[0], &ps[1]);
    b.engine.close().await;
    let opts = PublishOpts {
        targets: Some(vec![b.nid()]),
        push: true,
        ..PublishOpts::default()
    };
    let (p, took) = publish_with(a, "pushed", opts).await;
    assert!(p.pending.is_empty(), "a push is never cut short: {p:?}");
    assert_eq!(p.failed, vec![b.nid()], "{p:?}");
    assert!(
        took > Duration::from_secs(1),
        "the full attempt ran ({took:?}), not just `publish_wait`"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn closing_the_engine_cancels_a_pending_send() {
    let ps = group(vec![EngineConfig::default(); 2]).await;
    let (a, b) = (&ps[0], &ps[1]);
    b.engine.close().await;
    let (p, _) = publish(a, "left behind").await;
    assert_eq!(p.pending, vec![b.nid()], "{p:?}");
    // Sends are aborted (unit-tested in `background_sends`); what is left is the endpoint's own
    // close, which waits up to about 3 s for the dead peer's half-open connection.
    tokio::time::timeout(Duration::from_secs(10), a.engine.close())
        .await
        .expect("close hung on a pending send");
    // Publishing on a closed engine reports, never panics or starts a send.
    if let Ok(PublishOutcome::Sent(p)) = a
        .engine
        .publish_clip(ClipInput::text("after close"), PublishOpts::default())
        .await
    {
        assert!(p.delivered.is_empty() && p.pending.is_empty(), "{p:?}");
    }
}
