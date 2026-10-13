//! clip/1 authorization and hostile-input handling.

mod common;

use common::engine::{EngPeer, eng_peer, group};
use fsync_core::{
    CoreError, EngineConfig, EngineEvent, TransferError,
    clip::{ClipInput, PublishOpts, PublishOutcome, Published},
};
use fsync_proto::{
    ALPN_CLIP, ClipKind, ClipOffer, DeviceList, FetchTarget, FileMeta, Platform, RepMeta,
    frame::encode_frame, sign,
};
use std::time::Duration;
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
    let mut tx = conn.open_uni().await.unwrap();
    tx.write_all(&encode_frame(offer).unwrap()).await.unwrap();
    tx.finish().unwrap();
    // Let the receiver read before the connection can drop.
    tokio::time::sleep(Duration::from_millis(300)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn receiver_rejects_oversized_offer() {
    let cfg = EngineConfig {
        max_clip_bytes: 1 << 20,
        ..EngineConfig::default()
    };
    let mut ps = group(vec![cfg; 2]).await;
    let mut o = text_offer(ps[0].nid(), "x");
    o.reps[0].size = 2 << 20;
    o.inline = vec![None];
    raw_send(&ps[0], &ps[1], &o).await;
    ps[1]
        .wait(|e| matches!(e, EngineEvent::Error { .. }).then_some(()))
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn untrusted_peer_cannot_fetch() {
    let ps = group(vec![EngineConfig::default(); 2]).await;
    let mut input = ClipInput::text("secret");
    input.reps[0].bytes = common::engine::pseudo_random(2 << 20, 3);
    let p = sent(
        ps[0]
            .engine
            .publish_clip(input, PublishOpts::default())
            .await
            .unwrap_or_else(|e| panic!("{e:?}")),
    );
    // Outsider: its own account list that includes A, so it dials; A does not trust it.
    let o = eng_peer(EngineConfig::default()).await;
    let g = sign(
        o.id.signing_key(),
        &DeviceList::genesis(Uuid::new_v4(), o.nid(), "o".into(), Platform::Cli, 1),
    )
    .unwrap();
    o.trust.pin_genesis(&g).unwrap();
    let l = DeviceList::next_add(&g, o.nid(), ps[0].nid(), "a".into(), Platform::Cli, 2).unwrap();
    o.trust
        .ingest(&sign(o.id.signing_key(), &l).unwrap())
        .unwrap();
    o.node()
        .discovery
        .hint(&ps[0].nid(), None, &[ps[0].loopback()])
        .unwrap();
    let r = o
        .engine
        .fetch_blob_from(
            &ps[0].nid(),
            p.item_id,
            FetchTarget::Rep(0),
            2 << 20,
            [0; 32],
        )
        .await;
    assert!(r.is_err(), "outsider must not fetch");
}

#[tokio::test(flavor = "multi_thread")]
async fn peer_not_offered_the_item_is_denied() {
    let mut ps = group(vec![EngineConfig::default(); 3]).await;
    let mut input = ClipInput::text("for b only");
    let body = common::engine::pseudo_random(2 << 20, 5);
    input.reps[0].bytes = body.clone();
    let b = ps[1].nid();
    let p = sent(
        ps[0]
            .engine
            .publish_clip(
                input,
                PublishOpts {
                    targets: Some(vec![b]),
                    push: false,
                },
            )
            .await
            .unwrap(),
    );
    assert_eq!(p.delivered, vec![b]);
    let (_, offer) = remote_clip(&mut ps[1]).await;
    let m = &offer.reps[0];
    // C was not offered it: Denied, even though C is a trusted device.
    let r = ps[2]
        .engine
        .fetch_blob_from(&ps[0].nid(), p.item_id, FetchTarget::Rep(0), m.size, m.hash)
        .await;
    assert!(
        matches!(r, Err(CoreError::Transfer(TransferError::Denied))),
        "{r:?}"
    );
    assert_eq!(ps[1].engine.fetch_rep(p.item_id, 0).await.unwrap(), body);
}

#[tokio::test(flavor = "multi_thread")]
async fn malicious_rel_path_is_rejected() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    for bad in [
        "../evil.txt",
        "/etc/passwd",
        "a/../../b",
        "C:\\x",
        "con.txt",
        "a//b",
    ] {
        let o = ClipOffer {
            kind: ClipKind::Files,
            reps: vec![],
            inline: vec![],
            files: vec![FileMeta {
                file_id: 0,
                name: "evil".into(),
                rel_path: bad.into(),
                size: 1,
                hash: [0; 32],
                mtime_ms: 0,
            }],
            ..text_offer(ps[0].nid(), "")
        };
        raw_send(&ps[0], &ps[1], &o).await;
        ps[1]
            .wait(|e| matches!(e, EngineEvent::Error { .. }).then_some(()))
            .await;
        assert!(ps[1].engine.received_offer(&o.item_id).is_none(), "{bad}");
    }
    while let Ok(e) = ps[1].events.try_recv() {
        assert!(!matches!(
            e,
            EngineEvent::RemoteClip { .. } | EngineEvent::IncomingFiles { .. }
        ));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn offer_with_foreign_origin_is_rejected() {
    let mut ps = group(vec![EngineConfig::default(); 3]).await;
    let o = text_offer(ps[2].nid(), "spoof"); // A claims C as origin
    raw_send(&ps[0], &ps[1], &o).await;
    ps[1]
        .wait(|e| matches!(e, EngineEvent::Error { .. }).then_some(()))
        .await;
}
