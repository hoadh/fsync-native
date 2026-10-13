mod history_common;
use fsync_core::{
    EngineEvent,
    history::{Cursor, Direction, FileState, NewItem},
};
use history_common::*;
use uuid::Uuid;

#[tokio::test]
async fn wrong_key_is_an_error_and_the_right_key_still_recovers_the_history() {
    let tmp = tempfile::tempdir().unwrap();
    {
        let e = open(tmp.path(), &KEY);
        assert!(e.hist.record(text_item("secret one", 1)).await.unwrap());
    }
    // Same key: data survives a restart.
    let e = open(tmp.path(), &KEY);
    assert_eq!(e.hist.list(None, 10).await.unwrap().items.len(), 1);
    drop(e);

    // A different key (flaky keychain read) must not move or replace anything, however often.
    for _ in 0..3 {
        let (sink, mut events) = fsync_core::EventSink::channel();
        let store = blob_store(tmp.path(), &[9; 32]);
        let r =
            fsync_core::history::History::open(&tmp.path().join("data"), &[9; 32], store, &sink);
        assert!(matches!(r, Err(fsync_core::CoreError::HistoryKeyMismatch)));
        assert!(events.try_recv().is_err(), "no HistoryReset");
    }
    let names: Vec<_> = std::fs::read_dir(tmp.path().join("data"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        !names.iter().any(|n| n.starts_with("history.corrupt-")),
        "{names:?}"
    );

    // The right key opens the original again.
    let e = open(tmp.path(), &KEY);
    assert_eq!(e.hist.list(None, 10).await.unwrap().items.len(), 1);
}

#[tokio::test]
async fn garbage_file_is_reset_not_a_crash() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("data")).unwrap();
    std::fs::write(tmp.path().join("data/history.db"), pseudo(8192, 3)).unwrap();
    let mut e = open(tmp.path(), &KEY);
    assert!(matches!(
        e.events.try_recv(),
        Ok(EngineEvent::HistoryReset { .. })
    ));
    assert!(e.hist.record(text_item("ok", 1)).await.unwrap());
}

#[tokio::test]
async fn database_is_not_readable_as_plain_sqlite() {
    let tmp = tempfile::tempdir().unwrap();
    {
        let e = open(tmp.path(), &KEY);
        e.hist
            .record(text_item("needle-in-haystack", 1))
            .await
            .unwrap();
    }
    let db = tmp.path().join("data/history.db");
    let raw = std::fs::read(&db).unwrap();
    assert!(!raw.starts_with(b"SQLite format 3"));
    assert!(!raw.windows(6).any(|w| w == b"needle"));
    let plain = rusqlite::Connection::open(&db).unwrap();
    assert!(
        plain
            .query_row("SELECT count(*) FROM sqlite_master", [], |r| r
                .get::<_, i64>(0))
            .is_err()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn data_dir_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    open(tmp.path(), &KEY);
    let mode = std::fs::metadata(tmp.path().join("data"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[tokio::test]
async fn pagination_is_stable_with_equal_timestamps() {
    let tmp = tempfile::tempdir().unwrap();
    let e = open(tmp.path(), &KEY);
    let mut ids = vec![];
    for i in 0..25u64 {
        let it = text_item(&format!("item {i}"), 1000 + i / 5); // groups of equal timestamps
        ids.push(it.id);
        e.hist.record(it).await.unwrap();
    }
    let (mut seen, mut cur) = (vec![], None::<Cursor>);
    loop {
        let p = e.hist.list(cur, 10).await.unwrap();
        seen.extend(p.items.iter().map(|i| (i.created_ms, i.id)));
        match p.next {
            Some(n) => cur = Some(n),
            None => break,
        }
    }
    assert_eq!(seen.len(), 25);
    let mut sorted = seen.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(seen, sorted, "newest first, id breaks ties");
    let uniq: std::collections::HashSet<_> = seen.iter().map(|s| s.1).collect();
    assert_eq!(uniq.len(), 25);
}

#[tokio::test]
async fn fts_search_by_preview() {
    let tmp = tempfile::tempdir().unwrap();
    let e = open(tmp.path(), &KEY);
    let a = text_item("Hello wonderful world", 1);
    let b = text_item("goodbye moon", 2);
    let c = text_item("hello there", 3);
    let (ida, idc) = (a.id, c.id);
    for it in [a, b, c] {
        e.hist.record(it).await.unwrap();
    }
    let hits = |q: &'static str| {
        let h = e.hist.clone();
        async move { h.search(q, None, 10).await.unwrap().items }
    };
    let ids: Vec<Uuid> = hits("hell").await.iter().map(|i| i.id).collect();
    assert_eq!(ids, vec![idc, ida], "prefix match, newest first");
    assert_eq!(hits("hello world").await.len(), 1, "all words must match");
    assert!(hits("absent").await.is_empty());
    for nasty in ["\"", "(", "*", "a OR", "NEAR(", "x\" y"] {
        let _ = hits(nasty).await; // must not error
    }
    assert!(hits("   ").await.is_empty());
    e.hist.delete_item(ida).await.unwrap();
    assert_eq!(hits("hello").await.len(), 1, "deleted rows leave the index");
}

#[tokio::test]
async fn pin_clear_detail_and_marks() {
    let tmp = tempfile::tempdir().unwrap();
    let e = open(tmp.path(), &KEY);
    let mut it = blob_item(e.store.as_ref(), b"payload", 10);
    it.files.push(file_row([5; 32]));
    it.reps[0].present = false;
    let id = it.id;
    assert!(e.hist.record(it.clone()).await.unwrap());
    assert!(!e.hist.record(it).await.unwrap(), "duplicate id ignored");

    let mut sensitive: NewItem = text_item("password", 11);
    sensitive.sensitive = true;
    let sid = sensitive.id;
    assert!(!e.hist.record(sensitive).await.unwrap());
    assert!(e.hist.item(sid).await.unwrap().is_none());
    assert!(
        e.hist
            .search("password", None, 10)
            .await
            .unwrap()
            .items
            .is_empty()
    );

    assert!(e.hist.mark_rep_present(id, 0).await.unwrap());
    assert!(
        e.hist
            .set_file_local_path(id, 0, "/tmp/a.bin".into())
            .await
            .unwrap()
    );
    let d = e.hist.item(id).await.unwrap().unwrap();
    assert!(d.reps[0].present);
    assert_eq!(d.files[0].local_path.as_deref(), Some("/tmp/a.bin"));
    assert_eq!(d.files[0].state, FileState::Saved);
    assert_eq!(d.item.total_size, 7 + 10);
    assert_eq!(d.item.direction, Direction::In);

    let other = text_item("unpinned", 20);
    e.hist.record(other).await.unwrap();
    assert!(e.hist.set_pinned(id, true).await.unwrap());
    assert!(!e.hist.set_pinned(Uuid::new_v4(), true).await.unwrap());
    assert_eq!(e.hist.clear(true).await.unwrap(), 1);
    let left = e.hist.list(None, 10).await.unwrap().items;
    assert_eq!(left.len(), 1);
    assert!(left[0].pinned && left[0].id == id);
    assert_eq!(e.hist.clear(false).await.unwrap(), 1);
    assert!(e.hist.list(None, 10).await.unwrap().items.is_empty());
    assert!(
        e.store.list().unwrap().is_empty(),
        "blobs unlinked with the items"
    );
}

/// Damages the database file, then reopens with the key that wrote it.
fn corrupt_and_reopen(dir: &std::path::Path, seed: u64) -> std::path::PathBuf {
    let db = dir.join("data/history.db");
    std::fs::write(&db, history_common::pseudo(300_000, seed)).unwrap();
    let _ = std::fs::remove_file(dir.join("data/history.db-wal"));
    let mut e = open(dir, &KEY);
    let EngineEvent::HistoryReset { backup } = e.events.try_recv().expect("reset") else {
        panic!("wrong event")
    };
    std::thread::sleep(std::time::Duration::from_millis(5)); // distinct backup timestamps
    backup
}

fn backups(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir.join("data"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            n.starts_with("history.corrupt-") && !n.ends_with("-wal") && !n.ends_with("-shm")
        })
        .count()
}

#[tokio::test]
async fn backups_with_data_are_never_pruned_and_empty_ones_keep_the_newest_three() {
    let tmp = tempfile::tempdir().unwrap();
    drop(open(tmp.path(), &KEY)); // creates the db and records the key
    let first = corrupt_and_reopen(tmp.path(), 1);
    for seed in 2..=6 {
        corrupt_and_reopen(tmp.path(), seed);
    }
    // Every backup is 300 kB of "data", far above an empty database: all are kept.
    assert_eq!(backups(tmp.path()), 6);
    assert!(first.exists());

    // Backups that hold nothing beyond a fresh database go once more than three exist, when a
    // reset has just created a genuinely empty database.
    let tmp = tempfile::tempdir().unwrap();
    drop(open(tmp.path(), &KEY));
    for i in 0..5u64 {
        let p = tmp
            .path()
            .join(format!("data/history.corrupt-{}", 1000 + i));
        std::fs::write(p, b"").unwrap();
    }
    let newest = corrupt_and_reopen(tmp.path(), 9);
    assert_eq!(backups(tmp.path()), 3);
    assert!(newest.exists());
    assert!(tmp.path().join("data/history.corrupt-1004").exists());
    assert!(!tmp.path().join("data/history.corrupt-1000").exists());
}

#[tokio::test]
async fn a_big_live_database_never_causes_data_backups_to_be_pruned() {
    let tmp = tempfile::tempdir().unwrap();
    {
        let e = open(tmp.path(), &KEY);
        for i in 0..60u64 {
            let big = "x".repeat(2000) + &i.to_string();
            e.hist.record(text_item(&big, 1 + i)).await.unwrap();
        }
    }
    let len = |n: &str| std::fs::metadata(tmp.path().join(n)).map_or(0, |m| m.len());
    let live = len("data/history.db") + len("data/history.db-wal");
    // Five old backups, each smaller than the live database yet holding data.
    for i in 0..5u64 {
        let p = tmp
            .path()
            .join(format!("data/history.corrupt-{}", 1000 + i));
        std::fs::write(p, history_common::pseudo(20_000, i)).unwrap();
    }
    assert!(
        live > 20_000,
        "live database must dwarf the backups: {live}"
    );
    for _ in 0..3 {
        let e = open(tmp.path(), &KEY);
        assert_eq!(e.hist.list(None, 100).await.unwrap().items.len(), 60);
    }
    assert_eq!(backups(tmp.path()), 5, "nothing pruned outside a reset");
}

#[tokio::test]
async fn reset_history_clears_a_key_mismatch_and_a_new_key_opens() {
    let tmp = tempfile::tempdir().unwrap();
    {
        let e = open(tmp.path(), &KEY);
        e.hist.record(text_item("old", 1)).await.unwrap();
    }
    let dir = tmp.path().join("data");
    let try_open = |key: &[u8; 32]| {
        let (sink, _rx) = fsync_core::EventSink::channel();
        fsync_core::history::History::open(&dir, key, blob_store(tmp.path(), key), &sink)
    };
    assert!(matches!(
        try_open(&[9; 32]),
        Err(fsync_core::CoreError::HistoryKeyMismatch)
    ));
    fsync_core::history::reset_history(&dir).unwrap();
    assert_eq!(backups(tmp.path()), 1, "old history kept as a backup");
    assert!(!dir.join("history.keycheck").exists());
    let h = try_open(&[9; 32]).expect("new key opens after the reset");
    assert!(h.list(None, 10).await.unwrap().items.is_empty());
    assert!(fsync_core::history::reset_history(&tmp.path().join("nope")).is_ok());
}
