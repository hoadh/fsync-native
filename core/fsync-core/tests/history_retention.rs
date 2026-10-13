mod history_common;
use fsync_core::settings::Settings;
use history_common::*;
use std::{collections::HashSet, sync::Arc};

fn hash_of(b: &[u8]) -> [u8; 32] {
    *blake3::hash(b).as_bytes()
}

async fn with_settings(e: &Env, f: impl FnOnce(&mut Settings)) {
    let mut s = e.hist.settings().await.unwrap();
    f(&mut s);
    e.hist.set_settings(s).await.unwrap();
}

#[tokio::test]
async fn age_retention_spares_pinned_and_shared_blobs() {
    let tmp = tempfile::tempdir().unwrap();
    let e = open(tmp.path(), &KEY);
    let now = now_ms();
    let old = now - 40 * DAY_MS;
    let (old_unpinned, old_pinned, fresh) = (
        blob_item(e.store.as_ref(), b"old unpinned", old),
        blob_item(e.store.as_ref(), b"old pinned", old + 1),
        blob_item(e.store.as_ref(), b"fresh", now),
    );
    // Same bytes as the fresh item: the blob must survive its older twin.
    let twin = blob_item(e.store.as_ref(), b"fresh", old + 2);
    let ids = (old_unpinned.id, old_pinned.id, fresh.id, twin.id);
    for it in [old_unpinned, old_pinned, fresh, twin] {
        e.hist.record(it).await.unwrap();
    }
    e.hist.set_pinned(ids.1, true).await.unwrap();

    let rep = e.hist.run_retention(now).await.unwrap();
    assert_eq!(rep.items_deleted, 2);
    assert_eq!(rep.blobs_removed, 1, "only the unshared blob goes");
    assert!(rep.bytes_freed > 0);
    assert!(e.hist.item(ids.0).await.unwrap().is_none());
    assert!(e.hist.item(ids.3).await.unwrap().is_none());
    assert!(
        e.hist.item(ids.1).await.unwrap().is_some(),
        "pinned never deleted"
    );
    assert!(e.hist.item(ids.2).await.unwrap().is_some());
    assert!(
        e.store
            .size_of(&hash_of(b"old unpinned"))
            .unwrap()
            .is_none()
    );
    assert!(e.store.size_of(&hash_of(b"old pinned")).unwrap().is_some());
    assert!(e.store.size_of(&hash_of(b"fresh")).unwrap().is_some());
}

#[tokio::test]
async fn size_cap_drops_oldest_unpinned_first() {
    let tmp = tempfile::tempdir().unwrap();
    let e = open(tmp.path(), &KEY);
    let now = now_ms();
    let mut ids = vec![];
    for i in 0..5u64 {
        let it = blob_item(
            e.store.as_ref(),
            &pseudo(100_000, 2 * i + 1),
            now - (5 - i) * 1000,
        );
        ids.push(it.id);
        e.hist.record(it).await.unwrap();
    }
    e.hist.set_pinned(ids[0], true).await.unwrap(); // oldest, but pinned
    with_settings(&e, |s| s.retention_bytes = 250_000).await;
    let r = e.hist.run_retention(now).await.unwrap();
    assert_eq!(r.items_deleted, 3);

    let left: HashSet<_> = e
        .hist
        .list(None, 50)
        .await
        .unwrap()
        .items
        .iter()
        .map(|i| i.id)
        .collect();
    let stored: u64 = e.store.list().unwrap().iter().map(|b| b.size).sum();
    assert!(stored <= 250_000, "under the cap: {stored}");
    assert!(left.contains(&ids[0]), "pinned kept");
    assert!(left.contains(&ids[4]), "newest kept");
    assert!(!left.contains(&ids[1]), "oldest unpinned went first");
    assert_eq!(left.len(), 2);
}

#[tokio::test]
async fn size_cap_stops_when_only_pinned_remains() {
    let tmp = tempfile::tempdir().unwrap();
    let e = open(tmp.path(), &KEY);
    let it = blob_item(e.store.as_ref(), &pseudo(50_000, 1), now_ms());
    let id = it.id;
    e.hist.record(it).await.unwrap();
    e.hist.set_pinned(id, true).await.unwrap();
    with_settings(&e, |s| s.retention_bytes = 10).await;
    e.hist.run_retention(now_ms()).await.unwrap();
    assert!(e.hist.item(id).await.unwrap().is_some());
}

#[tokio::test]
async fn outbox_referenced_blob_is_never_deleted() {
    let tmp = tempfile::tempdir().unwrap();
    let e = open(tmp.path(), &KEY);
    let now = now_ms();
    let data = pseudo(30_000, 5);
    let h = hash_of(&data);
    let it = blob_item(e.store.as_ref(), &data, now - 60 * DAY_MS);
    let id = it.id;
    e.hist.record(it).await.unwrap();
    // An orphan the outbox serves (never in history) must survive the orphan sweep too.
    let orphan = pseudo(2_000, 6);
    let oh = hash_of(&orphan);
    e.store.put(&oh, &orphan).unwrap();
    let blob_path = |h: &[u8; 32]| {
        tmp.path()
            .join("blobs")
            .join(blake3::Hash::from_bytes(*h).to_hex().as_str())
    };
    let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(7 * 3600);
    std::fs::File::options()
        .write(true)
        .open(blob_path(&oh))
        .unwrap()
        .set_modified(long_ago)
        .unwrap();

    let guarded: Arc<HashSet<[u8; 32]>> = Arc::new([h, oh].into());
    e.hist.set_blob_guard(Arc::new(move || (*guarded).clone()));
    e.hist.run_retention(now).await.unwrap();
    assert!(e.hist.item(id).await.unwrap().is_none(), "row expired");
    assert!(e.store.size_of(&h).unwrap().is_some(), "outbox blob stays");
    assert!(
        e.store.size_of(&oh).unwrap().is_some(),
        "guarded orphan stays"
    );

    // Guard released: next run collects the orphans (grace passed for the old one).
    e.hist.set_blob_guard(Arc::new(HashSet::new));
    std::fs::File::options()
        .write(true)
        .open(blob_path(&h))
        .unwrap()
        .set_modified(long_ago)
        .unwrap();
    let rep = e.hist.run_retention(now).await.unwrap();
    assert_eq!(rep.blobs_removed, 2);
    assert!(e.store.list().unwrap().is_empty());
}

#[tokio::test]
async fn fresh_orphans_get_a_grace_period() {
    let tmp = tempfile::tempdir().unwrap();
    let e = open(tmp.path(), &KEY);
    let h = hash_of(b"in flight");
    e.store.put(&h, b"in flight").unwrap(); // put but not yet recorded
    e.hist.run_retention(now_ms()).await.unwrap();
    assert!(e.store.size_of(&h).unwrap().is_some());
}

#[tokio::test]
async fn delete_item_respects_guard_and_sharing() {
    let tmp = tempfile::tempdir().unwrap();
    let e = open(tmp.path(), &KEY);
    let (a, b) = (
        blob_item(e.store.as_ref(), b"shared", 1),
        blob_item(e.store.as_ref(), b"shared", 2),
    );
    let (ia, ib) = (a.id, b.id);
    e.hist.record(a).await.unwrap();
    e.hist.record(b).await.unwrap();
    e.hist.delete_item(ia).await.unwrap();
    assert!(e.store.size_of(&hash_of(b"shared")).unwrap().is_some());
    let h = hash_of(b"shared");
    e.hist.set_blob_guard(Arc::new(move || [h].into()));
    e.hist.delete_item(ib).await.unwrap();
    assert!(e.store.size_of(&h).unwrap().is_some(), "guard wins");
    assert!(!e.hist.delete_item(ib).await.unwrap());
}

#[tokio::test]
async fn size_cap_skips_items_that_free_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let e = open(tmp.path(), &KEY);
    let now = now_ms();
    // Oldest: inline-only text (no stored bytes). Then two items sharing one blob, then a big one.
    let text = text_item("inline only", now - 10_000);
    let text_id = text.id;
    e.hist.record(text).await.unwrap();
    let shared = pseudo(60_000, 3);
    let a = blob_item(e.store.as_ref(), &shared, now - 9_000);
    let b = blob_item(e.store.as_ref(), &shared, now - 8_000);
    let (a_id, b_id) = (a.id, b.id);
    e.hist.record(a).await.unwrap();
    e.hist.record(b).await.unwrap();
    let big = blob_item(e.store.as_ref(), &pseudo(200_000, 4), now - 7_000);
    let big_id = big.id;
    e.hist.record(big).await.unwrap();
    with_settings(&e, |s| s.retention_bytes = 100_000).await;
    e.hist.run_retention(now).await.unwrap();
    let has = |id| {
        let h = e.hist.clone();
        async move { h.item(id).await.unwrap().is_some() }
    };
    assert!(
        has(text_id).await,
        "inline-only item is not eaten for nothing"
    );
    assert!(!has(big_id).await, "the cap is still enforced past it");
    // Shared content stays referenced by both items.
    assert!(has(a_id).await && has(b_id).await);
    let stored: u64 = e.store.list().unwrap().iter().map(|b| b.size).sum();
    assert!(stored <= 100_000, "{stored}");
}

#[tokio::test]
async fn size_cap_leaves_items_whose_blob_the_outbox_protects() {
    let tmp = tempfile::tempdir().unwrap();
    let e = open(tmp.path(), &KEY);
    let now = now_ms();
    let guarded = blob_item(e.store.as_ref(), &pseudo(150_000, 5), now - 9_000);
    let (gid, gh) = (guarded.id, hash_of(&pseudo(150_000, 5)));
    let plain = blob_item(e.store.as_ref(), &pseudo(150_000, 6), now - 8_000);
    let pid = plain.id;
    e.hist.record(guarded).await.unwrap();
    e.hist.record(plain).await.unwrap();
    e.hist.set_blob_guard(Arc::new(move || [gh].into()));
    with_settings(&e, |s| s.retention_bytes = 100_000).await;
    e.hist.run_retention(now).await.unwrap();
    assert!(e.hist.item(gid).await.unwrap().is_some(), "protected stays");
    assert!(e.hist.item(pid).await.unwrap().is_none(), "next one goes");
}
