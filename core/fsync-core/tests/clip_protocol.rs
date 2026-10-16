//! clip/2 over real iroh endpoints on loopback.

mod common;

use common::engine::{EngPeer, group};
use fsync_core::{
    CoreError, EngineConfig, EngineEvent,
    clip::{ClipInput, PublishOpts, PublishOutcome, Published},
};
use fsync_proto::{ALPN_CLIP, ClipKind, ClipOffer, RepMeta, frame::encode_frame};
use std::time::{Duration, Instant};
use uuid::Uuid;

async fn remote_clip(p: &mut EngPeer) -> (Uuid, fsync_core::OfferRef) {
    p.wait(|e| match e {
        EngineEvent::RemoteClip { offer, .. } => Some((offer.item_id, offer.clone())),
        _ => None,
    })
    .await
}

fn sent(o: PublishOutcome) -> Published {
    match o {
        PublishOutcome::Sent(p) => p,
        PublishOutcome::Echo => panic!("unexpected echo"),
    }
}

fn text_offer(origin: fsync_proto::NodeId, s: &str) -> ClipOffer {
    ClipOffer {
        v: 1,
        item_id: Uuid::now_v7(),
        origin,
        ts_ms: 1,
        kind: ClipKind::Text,
        reps: vec![RepMeta {
            mime: "text/plain".into(),
            size: s.len() as u64,
            hash: *blake3::hash(s.as_bytes()).as_bytes(),
        }],
        inline: vec![Some(s.as_bytes().to_vec())],
        files: vec![],
        preview: None,
        push: false,
        sensitive: false,
    }
}

async fn raw_send(from: &EngPeer, to: &EngPeer, offer: &ClipOffer) {
    let conn = from
        .node()
        .connect(&to.nid(), ALPN_CLIP)
        .await
        .expect("connect");
    let (mut tx, _rx) = conn.open_bi().await.unwrap();
    tx.write_all(&encode_frame(offer).unwrap()).await.unwrap();
    tx.finish().unwrap();
    // Let the receiver read before the connection can drop.
    tokio::time::sleep(Duration::from_millis(300)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn inline_text_arrives_fast_and_intact() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    // Warm the connection like a long-lived session would be.
    let warm = sent(
        ps[0]
            .engine
            .publish_clip(ClipInput::text("warm"), PublishOpts::default())
            .await
            .unwrap(),
    );
    assert_eq!(warm.delivered.len(), 1);
    remote_clip(&mut ps[1]).await;
    let mut best = Duration::MAX;
    for i in 0..5 {
        let t = Instant::now();
        let text = format!("hello {i}");
        ps[0]
            .engine
            .publish_clip(ClipInput::text(&text), PublishOpts::default())
            .await
            .unwrap();
        let (id, offer) = remote_clip(&mut ps[1]).await;
        let dt = t.elapsed();
        best = best.min(dt);
        println!("clip inline latency #{i}: {dt:?}");
        assert!(dt < Duration::from_millis(300), "{dt:?}");
        assert_eq!(offer.origin, ps[0].nid());
        assert_eq!(
            ps[1].engine.fetch_rep(id, 0).await.unwrap(),
            text.as_bytes()
        );
    }
    println!("clip inline latency best: {best:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn duplicate_item_is_delivered_once() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    let o = text_offer(ps[0].nid(), "dup");
    raw_send(&ps[0], &ps[1], &o).await;
    raw_send(&ps[0], &ps[1], &o).await;
    remote_clip(&mut ps[1]).await;
    while let Ok(e) = ps[1].events.try_recv() {
        assert!(
            !matches!(e, EngineEvent::RemoteClip { .. }),
            "delivered twice"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn echo_guard_suppresses_reflected_content_only() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    ps[0]
        .engine
        .publish_clip(ClipInput::text("ping"), PublishOpts::default())
        .await
        .unwrap();
    remote_clip(&mut ps[1]).await;
    // B's clipboard now holds "ping" because we applied it: re-publishing it is an echo.
    let o = ps[1]
        .engine
        .publish_clip(ClipInput::text("ping"), PublishOpts::default())
        .await
        .unwrap();
    assert!(matches!(o, PublishOutcome::Echo));
    // Genuinely new content goes out, and after that "ping" is a real copy again.
    sent(
        ps[1]
            .engine
            .publish_clip(ClipInput::text("pong"), PublishOpts::default())
            .await
            .unwrap(),
    );
    remote_clip(&mut ps[0]).await;
    let o = ps[1]
        .engine
        .publish_clip(ClipInput::text("ping"), PublishOpts::default())
        .await
        .unwrap();
    assert!(matches!(o, PublishOutcome::Sent(_)));
}

#[tokio::test(flavor = "multi_thread")]
async fn oversized_clip_is_too_large() {
    let cfg = EngineConfig {
        max_clip_bytes: 1 << 20,
        ..EngineConfig::default()
    };
    let ps = group(vec![cfg; 2]).await;
    let big = "x".repeat(2 << 20);
    let r = ps[0]
        .engine
        .publish_clip(ClipInput::text(&big), PublishOpts::default())
        .await;
    assert!(matches!(r, Err(CoreError::TooLarge { .. })), "{r:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn spilled_rep_is_fetched_by_hash() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    let big = common::engine::pseudo_random(3 << 20, 7);
    let mut input = ClipInput::text("big");
    input.reps[0].bytes = big.clone();
    input.reps[0].mime = "application/octet-stream".into();
    ps[0]
        .engine
        .publish_clip(input, PublishOpts::default())
        .await
        .unwrap();
    let (id, offer) = remote_clip(&mut ps[1]).await;
    assert!(offer.inline[0].is_none());
    assert_eq!(ps[1].engine.fetch_rep(id, 0).await.unwrap(), big);
}
