//! Engine <-> history wiring over real iroh endpoints on loopback.

mod common;
mod history_common;

use common::{
    engine::{EngPeer, group_with, pseudo_random},
    wait_on,
};
use fsync_core::{
    EngineConfig, EngineEvent,
    clip::{ClipInput, FileInput, PublishOpts, PublishOutcome, RepInput},
    history::{Direction, FileState},
};
use fsync_proto::ClipKind;
use uuid::Uuid;

async fn pair() -> (EngPeer, EngPeer) {
    let mut g = group_with(vec![EngineConfig::default(), EngineConfig::default()], true).await;
    let b = g.pop().unwrap();
    (g.pop().unwrap(), b)
}

async fn remote(p: &mut EngPeer) -> Uuid {
    wait_on(&mut p.events, |e| match e {
        EngineEvent::RemoteClip { offer, .. } => Some(offer.item_id),
        _ => None,
    })
    .await
}

fn item_id(o: PublishOutcome) -> Uuid {
    match o {
        PublishOutcome::Sent(p) => p.item_id,
        PublishOutcome::Echo => panic!("echo"),
    }
}

fn on_disk_contains(p: &EngPeer, needle: &[u8]) -> bool {
    std::fs::read_dir(p.blobs_dir()).unwrap().any(|f| {
        let raw = std::fs::read(f.unwrap().path()).unwrap();
        raw.windows(needle.len()).any(|w| w == needle)
    })
}

#[tokio::test]
async fn text_clip_is_recorded_on_both_sides() {
    let (a, mut b) = pair().await;
    let text = "history wiring check: unique phrase 8731";
    let id = item_id(
        a.engine
            .publish_clip(ClipInput::text(text), PublishOpts::default())
            .await
            .unwrap(),
    );
    assert_eq!(remote(&mut b).await, id);

    let (ha, hb) = (a.history.as_ref().unwrap(), b.history.as_ref().unwrap());
    let sent = ha.item(id).await.unwrap().expect("sender row");
    assert_eq!(sent.item.direction, Direction::Out);
    assert_eq!(
        sent.item.peer,
        Some(b.nid()),
        "single recipient is recorded"
    );
    assert!(sent.reps[0].present);
    let got = hb.item(id).await.unwrap().expect("receiver row");
    assert_eq!(got.item.direction, Direction::In);
    assert_eq!(got.item.peer, Some(a.nid()));
    assert_eq!(got.item.preview.as_deref(), Some(text));
    assert!(got.reps[0].present, "inline rep stored on arrival");
    assert_eq!(
        hb.search("unique 8731", None, 10)
            .await
            .unwrap()
            .items
            .len(),
        1
    );

    // Content is retrievable from the (encrypted) blob store, and never plaintext on disk.
    let mut buf = vec![0u8; text.len()];
    let store = fsync_core::blobs::EncryptedBlobStore::new(
        std::sync::Arc::new(fsync_core::blobs::FsBlobStore::open(b.blobs_dir()).unwrap()),
        &[5; 32],
    )
    .unwrap();
    use fsync_core::blobs::BlobStore;
    assert_eq!(
        store.read_at(&got.reps[0].hash, 0, &mut buf).unwrap(),
        text.len()
    );
    assert_eq!(buf, text.as_bytes());
    assert!(!on_disk_contains(&a, b"unique phrase"));
    assert!(!on_disk_contains(&b, b"unique phrase"));
}

#[tokio::test]
async fn large_rep_becomes_present_after_fetch() {
    let (a, mut b) = pair().await;
    let big = pseudo_random(2 << 20, 11);
    let input = ClipInput {
        kind: ClipKind::Image,
        reps: vec![RepInput {
            mime: "image/png".into(),
            bytes: big.clone(),
        }],
        files: vec![],
        preview: Some("big picture".into()),
        sensitive: false,
    };
    let id = item_id(
        a.engine
            .publish_clip(input, PublishOpts::default())
            .await
            .unwrap(),
    );
    assert_eq!(remote(&mut b).await, id);
    let hb = b.history.as_ref().unwrap();
    assert!(!hb.item(id).await.unwrap().unwrap().reps[0].present);
    assert_eq!(b.engine.fetch_rep(id, 0).await.unwrap(), big);
    assert!(hb.item(id).await.unwrap().unwrap().reps[0].present);
    let ha = a.history.as_ref().unwrap();
    assert!(
        ha.item(id).await.unwrap().unwrap().reps[0].present,
        "spilled rep recorded"
    );
}

#[tokio::test]
async fn sensitive_clip_is_not_stored_by_sender() {
    let (a, mut b) = pair().await;
    let input = ClipInput::text("hunter2-concealed").sensitive();
    let id = item_id(
        a.engine
            .publish_clip(input, PublishOpts::default())
            .await
            .unwrap(),
    );
    remote(&mut b).await;
    // The receiver keeps nothing of it either.
    let hb = b.history.as_ref().unwrap();
    assert!(hb.item(id).await.unwrap().is_none());
    assert!(hb.list(None, 10).await.unwrap().items.is_empty());
    assert!(!on_disk_contains(&b, b"hunter2-concealed"));
    let ha = a.history.as_ref().unwrap();
    assert!(ha.item(id).await.unwrap().is_none());
    assert!(ha.list(None, 10).await.unwrap().items.is_empty());
    assert!(
        ha.search("hunter2", None, 10)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(!on_disk_contains(&a, b"hunter2-concealed"));
}

#[tokio::test]
async fn saved_file_records_local_path() {
    let (a, mut b) = pair().await;
    let src = a.dir.path().join("note.txt");
    std::fs::write(&src, b"file body").unwrap();
    let input = ClipInput::files(vec![FileInput::new(&src)]);
    let id = item_id(
        a.engine
            .publish_clip(input, PublishOpts::default())
            .await
            .unwrap(),
    );
    assert_eq!(remote(&mut b).await, id);
    let hb = b.history.as_ref().unwrap();
    assert_eq!(
        hb.item(id).await.unwrap().unwrap().files[0].state,
        FileState::Offered
    );

    let dest_dir = b.dir.path().join("downloads");
    std::fs::create_dir_all(&dest_dir).unwrap();
    let placed = b.engine.fetch_file_to(id, 0, &dest_dir).await.unwrap();
    let f = &hb.item(id).await.unwrap().unwrap().files[0];
    assert_eq!(f.state, FileState::Saved);
    assert_eq!(
        f.local_path.as_deref(),
        Some(placed.to_string_lossy().as_ref())
    );
    assert_eq!(
        std::fs::read(placed).unwrap(),
        b"file body",
        "saved files stay plaintext"
    );
}
