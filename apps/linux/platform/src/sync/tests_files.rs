use super::tests_rig::*;
use crate::clipboard::formats::parse_uri_list;
use crate::clipboard::{KDE_HINT_MIME, MARKER_MIME};
use crate::files::EAGER_LIMIT;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

const ID: &str = "0b1c2d3e-aaaa-bbbb-cccc-0123456789ab";

#[test]
fn a_small_file_clip_lands_in_the_inbox_and_is_offered_as_file_references() {
    let r = rig_with(true, true, false, Some(Arc::new(|_| {})));
    r.engine
        .on_remote_clip(file_item(ID, &[("a.txt", 10), ("dir/b.txt", 10)], &r));
    let entry = wait(|| r.backend.written.lock().expect("w").first().cloned());
    let uris = &entry.find("text/uri-list").expect("uri-list").data;
    let paths = parse_uri_list(uris, false);
    assert_eq!(paths.len(), 2, "a folder is one entry");
    assert!(
        paths
            .iter()
            .all(|p| p.starts_with(r.home.join("inbox").join(ID).canonicalize().expect("canon")))
    );
    let gnome = &entry
        .find("x-special/gnome-copied-files")
        .expect("gnome")
        .data;
    assert!(gnome.starts_with(b"copy\nfile://"));
    let names = entry.offered_names();
    assert!(names.iter().any(|n| n == MARKER_MIME) && names.iter().any(|n| n == KDE_HINT_MIME));
    assert_eq!(r.engine.current_file_item().as_deref(), Some(ID));
    assert!(r.home.join("inbox").join(ID).join("dir/b.txt").exists());
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn a_large_file_clip_is_not_downloaded_and_the_hook_hears_of_it() {
    let (tx, rx) = std::sync::mpsc::channel();
    let hook: super::LargeHook = Arc::new(move |i| tx.send(i.item_id.clone()).expect("send"));
    let r = rig_with(true, true, false, Some(hook));
    r.engine
        .on_remote_clip(file_item(ID, &[("big.bin", EAGER_LIMIT + 1)], &r));
    assert_eq!(rx.recv_timeout(Duration::from_secs(2)).as_deref(), Ok(ID));
    wait(|| {
        r.engine
            .status()
            .last_event
            .contains("Large file clip")
            .then_some(())
    });
    assert_eq!(r.service.file_fetches.load(Ordering::SeqCst), 0);
    assert!(r.backend.written.lock().expect("w").is_empty());
    assert!(!r.home.join("inbox").exists());
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn a_failed_download_is_reported_and_leaves_nothing() {
    let r = rig_with(true, true, false, Some(Arc::new(|_| {})));
    let mut it = file_item(ID, &[("a.txt", 10)], &r);
    it.item_id = "0b1c2d3e-0000-0000-0000-000000000001".into(); // unknown to the fake: "gone"
    r.engine.on_remote_clip(it);
    wait(|| {
        r.engine
            .status()
            .last_event
            .contains("Could not receive")
            .then_some(())
    });
    assert!(r.backend.written.lock().expect("w").is_empty());
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn in_manual_mode_the_files_wait_as_pending_until_paste() {
    let r = rig_with(true, false, false, Some(Arc::new(|_| {})));
    r.engine.on_remote_clip(file_item(ID, &[("a.txt", 10)], &r));
    wait(|| r.engine.status().pending.map(|_| ()));
    assert!(r.backend.written.lock().expect("w").is_empty());
    assert!(matches!(r.engine.paste(), super::Paste::Done));
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn copy_back_writes_through_the_backend_or_asks_for_a_window() {
    let r = rig(true, true, false);
    let e = crate::clipboard::formats::files_entry(&["/tmp/x".into()]);
    assert!(matches!(
        r.engine.copy_back(e.clone(), Some(ID.into())),
        super::Paste::Done
    ));
    assert_eq!(r.backend.written.lock().expect("w").len(), 1);
    assert_eq!(r.engine.current_file_item().as_deref(), Some(ID));
    r.engine.shutdown(Duration::from_secs(1));
    let manual = rig(false, true, false);
    assert!(matches!(
        manual.engine.copy_back(e, None),
        super::Paste::NeedsWindow(_)
    ));
    manual.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn an_unsafe_file_name_is_refused_before_any_download() {
    let r = rig_with(true, true, false, Some(Arc::new(|_| {})));
    r.engine
        .on_remote_clip(file_item(ID, &[("../escape.bin", 10)], &r));
    wait(|| {
        r.engine
            .status()
            .last_event
            .contains("Could not receive")
            .then_some(())
    });
    assert_eq!(r.service.file_fetches.load(Ordering::SeqCst), 0);
    assert!(r.backend.written.lock().expect("w").is_empty());
    assert!(!r.home.join("inbox").join("escape.bin").exists());
    assert!(!r.home.join("inbox").join(ID).exists());
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn exactly_the_eager_limit_is_still_downloaded() {
    let r = rig_with(true, true, false, Some(Arc::new(|_| {})));
    r.engine
        .on_remote_clip(file_item(ID, &[("edge.bin", EAGER_LIMIT)], &r));
    let entry = wait(|| r.backend.written.lock().expect("w").first().cloned());
    assert!(entry.find("text/uri-list").is_some());
    assert_eq!(r.service.file_fetches.load(Ordering::SeqCst), 1);
    assert!(!r.engine.status().last_event.contains("Large file clip"));
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn a_newer_file_clip_cancels_the_older_download() {
    let r = rig_with(true, true, false, Some(Arc::new(|_| {})));
    *r.service.fetch_delay.lock().expect("d") = Duration::from_millis(200);
    let old = "0b1c2d3e-0000-0000-0000-00000000000a";
    let new = "0b1c2d3e-0000-0000-0000-00000000000b";
    r.engine
        .on_remote_clip(file_item(old, &[("a.bin", 10), ("b.bin", 10)], &r));
    // The older download is inside its first file fetch (200 ms) when the newer clip arrives.
    std::thread::sleep(Duration::from_millis(100));
    r.engine
        .on_remote_clip(file_item(new, &[("c.bin", 10)], &r));
    let entry = wait(|| r.backend.written.lock().expect("w").first().cloned());
    assert_eq!(
        r.backend.written.lock().expect("w").len(),
        1,
        "only the newer clip reaches the clipboard"
    );
    let paths = parse_uri_list(&entry.find("text/uri-list").expect("uri-list").data, false);
    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("c.bin"), "{paths:?}");
    assert!(
        !r.home.join("inbox").join(old).exists(),
        "the cancelled copy is removed"
    );
    assert_eq!(r.engine.current_file_item().as_deref(), Some(new));
    r.engine.shutdown(Duration::from_secs(1));
}
