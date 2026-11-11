//! An offline account device must not slow down publishing to the others: `publish` returns after
//! `publish_wait` and the dead peer is tried in the background. A device that comes back gets the
//! next clip, a slow but live peer gets the newest clip and never an older one after it (a
//! superseded clip's offer is withdrawn), a live peer is never reported busy after its result, an
//! explicit push still waits for the real result, a copy burst behind a dead peer holds at most
//! two clip frames, and closing the engine with a send still running neither hangs nor panics.

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
const QUICK: Duration = Duration::from_secs(3);
/// A publish that must not wait at all (the dead peer's lane is busy): below `publish_wait`.
const NO_WAIT: Duration = Duration::from_millis(1250);

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

/// Item ids of every clip `p` receives until `until`.
async fn received_until(p: &mut EngPeer, until: Instant) -> Vec<Uuid> {
    let mut ids = vec![];
    let _ = tokio::time::timeout_at(until.into(), async {
        loop {
            if let EngineEvent::RemoteClip { offer, .. } = p.events.recv().await.unwrap() {
                ids.push(offer.item_id);
            }
        }
    })
    .await;
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
            assert!(took < NO_WAIT, "clip {i}: waited {took:?}");
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

    let away_at = Instant::now();
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
    // Keep listening until the "while away" attempt (2 s + 5 s retry) has surely ended: if its
    // retry could still land after `next`, it would show up here.
    let quiet = (away_at + Duration::from_secs(8)).max(Instant::now() + Duration::from_secs(1));
    got.extend(received_until(&mut back, quiet).await);
    // The older clip, if its retry reached the restarted device, came first.
    assert_eq!(
        got.last(),
        Some(&next.item_id),
        "older clip after the newest: {got:?}"
    );
    assert!(
        got.iter()
            .all(|id| *id == away.item_id || *id == next.item_id),
        "{got:?}"
    );
    back.engine.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn slow_live_peer_gets_the_newest_clip_never_an_older_one_after_it() {
    // b answers every offer after 1 s (below the 2 s first attempt, so nothing is retried);
    // a stops waiting after 200 ms, so every send to b outlives its publish.
    let delay = Duration::from_secs(1);
    let a_cfg = EngineConfig {
        publish_wait: Duration::from_millis(200),
        ..EngineConfig::default()
    };
    let b_cfg = EngineConfig {
        clip_accept_delay: Some(delay),
        ..EngineConfig::default()
    };
    let mut ps = group(vec![a_cfg, b_cfg]).await;
    let (n, took) = publish(&ps[0], "n").await;
    assert_eq!(
        n.pending,
        vec![ps[1].nid()],
        "b is slow, not offline: {n:?}"
    );
    assert!(took < delay, "publish waited for the slow peer ({took:?})");
    // n's attempt holds b's lane: both are queued behind it, and n+1 is superseded by n+2.
    let (n1, took1) = publish(&ps[0], "n+1").await;
    let (n2, took2) = publish(&ps[0], "n+2").await;
    for (p, t) in [(&n1, took1), (&n2, took2)] {
        assert_eq!(p.pending, vec![ps[1].nid()], "{p:?}");
        assert!(p.failed.is_empty(), "queued, not failed: {p:?}");
        assert!(t < delay, "a busy lane is not waited for ({t:?})");
    }
    // n+1 was superseded at once: its offer to b is withdrawn, n+2's stays.
    let a = &ps[0].engine;
    assert_eq!(
        a.clip_offered_to(&n1.item_id),
        Some(vec![]),
        "n+1 unoffered"
    );
    assert_eq!(a.clip_offered_to(&n2.item_id), Some(vec![ps[1].nid()]));
    let b = &mut ps[1];
    let mut got = received(b, 2, Duration::from_secs(10)).await;
    // Twice the delay: room for n+1 to be (wrongly) sent after n+2.
    got.extend(received_until(b, Instant::now() + 2 * delay + Duration::from_millis(500)).await);
    assert_eq!(
        got,
        vec![n.item_id, n2.item_id],
        "want n then n+2 (n+1 skipped); n={} n+1={} n+2={}",
        n.item_id,
        n1.item_id,
        n2.item_id
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_burst_to_a_dead_peer_holds_at_most_two_frames() {
    // A long retry (2 s + 30 s): the first clip's attempt runs for the whole burst.
    let cfg = EngineConfig {
        send_timeout: Duration::from_secs(30),
        publish_wait: Duration::from_millis(200),
        ..EngineConfig::default()
    };
    let ps = group(vec![cfg; 2]).await;
    let (a, b) = (&ps[0], &ps[1]);
    b.engine.close().await;
    for i in 0..50 {
        let (p, _) = publish(a, &format!("burst {i}")).await;
        assert_eq!(p.pending, vec![b.nid()], "clip {i}: {p:?}");
    }
    // The running attempt's frame and the newest waiting one; the 48 in between were dropped.
    let held = a.engine.clip_frames_held();
    assert!(held <= 2, "{held} frames held for a dead peer");
    a.engine.close().await;
    assert_eq!(a.engine.clip_frames_held(), 0, "close frees every frame");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_live_peer_is_never_busy_once_its_result_is_reported() {
    // The lane runner stalls 1 s right after reporting each result. If the lane were still marked
    // running then, the next publish would find the live peer "busy" and report it pending.
    let a_cfg = EngineConfig {
        lane_report_pause: Some(Duration::from_secs(1)),
        ..EngineConfig::default()
    };
    let ps = group(vec![a_cfg, EngineConfig::default()]).await;
    let (a, b) = (&ps[0], &ps[1]);
    for i in 0..3 {
        let (p, _) = publish(a, &format!("live {i}")).await;
        assert_eq!(p.delivered, vec![b.nid()], "clip {i}: {p:?}");
        assert!(
            p.pending.is_empty(),
            "clip {i}: live peer reported pending: {p:?}"
        );
    }
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
    // A long retry (2 s + 30 s): close only returns well before that if the send is aborted.
    let cfg = EngineConfig {
        send_timeout: Duration::from_secs(30),
        ..EngineConfig::default()
    };
    let ps = group(vec![cfg; 2]).await;
    let (a, b) = (&ps[0], &ps[1]);
    b.engine.close().await;
    let (p, _) = publish(a, "left behind").await;
    assert_eq!(p.pending, vec![b.nid()], "{p:?}");
    // What is left after the abort is the endpoint's own close, which waits up to about 3 s for
    // the dead peer's half-open connection.
    tokio::time::timeout(Duration::from_secs(10), a.engine.close())
        .await
        .expect("close waited for the pending send instead of aborting it");
    // Publishing on a closed engine reports the peer as failed: no send is started, no panic.
    let after = a
        .engine
        .publish_clip(ClipInput::text("after close"), PublishOpts::default())
        .await;
    match after {
        Ok(PublishOutcome::Sent(p)) => {
            assert!(p.delivered.is_empty() && p.pending.is_empty(), "{p:?}");
            assert_eq!(p.failed, vec![b.nid()], "{p:?}");
        }
        other => panic!("publish after close: {other:?}"),
    }
}
