//! A sensitive fetch cleans up its blob, unless other content still uses the same bytes.

mod common;
mod history_common;

use common::{
    engine::{EngPeer, group_with, pseudo_random},
    wait_on,
};
use fsync_core::{
    EngineConfig, EngineEvent,
    clip::{ClipInput, PublishOpts, PublishOutcome, RepInput},
};
use fsync_proto::ClipKind;
use uuid::Uuid;

fn image(bytes: &[u8], sensitive: bool) -> ClipInput {
    ClipInput {
        kind: ClipKind::Image,
        reps: vec![RepInput {
            mime: "image/png".into(),
            bytes: bytes.to_vec(),
        }],
        files: vec![],
        preview: None,
        sensitive,
    }
}

async fn send_and_receive(a: &EngPeer, b: &mut EngPeer, input: ClipInput) -> Uuid {
    let PublishOutcome::Sent(p) = a
        .engine
        .publish_clip(input, PublishOpts::default())
        .await
        .unwrap()
    else {
        panic!("echo")
    };
    wait_on(&mut b.events, |e| match e {
        EngineEvent::RemoteClip { offer, .. } if offer.item_id == p.item_id => Some(()),
        _ => None,
    })
    .await;
    p.item_id
}

/// Blobs of at least 1 MiB in the store directory.
fn big_blobs(p: &EngPeer) -> usize {
    std::fs::read_dir(p.blobs_dir())
        .unwrap()
        .filter(|f| f.as_ref().unwrap().metadata().unwrap().len() >= 1 << 20)
        .count()
}

#[tokio::test]
async fn sensitive_fetch_keeps_a_blob_that_history_references() {
    let mut g = group_with(vec![EngineConfig::default(), EngineConfig::default()], true).await;
    let mut b = g.pop().unwrap();
    let a = g.pop().unwrap();
    let big = pseudo_random(2 << 20, 5);

    // Ordinary item: B fetches it and history references the blob.
    let plain = send_and_receive(&a, &mut b, image(&big, false)).await;
    assert_eq!(b.engine.fetch_rep(plain, 0).await.unwrap(), big);
    assert_eq!(big_blobs(&b), 1);

    // Same bytes arrive again as a sensitive item. Fetching it must not delete the shared blob.
    let secret = send_and_receive(&a, &mut b, image(&big, true)).await;
    assert_eq!(b.engine.fetch_rep(secret, 0).await.unwrap(), big);
    assert_eq!(big_blobs(&b), 1, "blob still referenced by history");
}

#[tokio::test]
async fn sensitive_fetch_removes_an_unreferenced_blob() {
    let mut g = group_with(vec![EngineConfig::default(), EngineConfig::default()], true).await;
    let mut b = g.pop().unwrap();
    let a = g.pop().unwrap();
    let big = pseudo_random(2 << 20, 6);
    let secret = send_and_receive(&a, &mut b, image(&big, true)).await;
    assert_eq!(b.engine.fetch_rep(secret, 0).await.unwrap(), big);
    assert_eq!(big_blobs(&b), 0);
}
