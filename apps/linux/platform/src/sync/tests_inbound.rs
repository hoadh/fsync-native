use super::tests_rig::*;
use crate::clipboard::{KDE_HINT_MIME, MARKER_MIME};
use fsync_proto::ClipKind;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[test]
fn inbound_is_fetched_eagerly_and_written_with_marker_and_hint() {
    let r = rig(true, true, false);
    let it = item(
        "a",
        ClipKind::Rich,
        &[("text/html", b"<i>x</i>"), ("text/plain", b"x")],
        &r,
    );
    r.engine.on_remote_clip(it);
    let entry = wait(|| r.backend.written.lock().expect("w").first().cloned());
    assert_eq!(
        r.service.fetched.load(Ordering::SeqCst),
        2,
        "every rep fetched before the write"
    );
    let names = entry.offered_names();
    for n in [
        "text/html",
        "UTF8_STRING",
        "STRING",
        MARKER_MIME,
        KDE_HINT_MIME,
    ] {
        assert!(names.iter().any(|x| x == n), "{n}");
    }
    assert!(
        r.engine
            .status()
            .last_event
            .contains("Received rich text from phone")
    );
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn echo_of_a_written_clip_is_not_sent_back_even_without_the_marker() {
    let r = rig(true, true, false);
    r.engine
        .on_remote_clip(item("a", ClipKind::Text, &[("text/plain", b"hello")], &r));
    wait(|| r.backend.written.lock().expect("w").first().cloned());
    // An owner that drops the marker reports the same content.
    let (c, _) = change(&[("UTF8_STRING", b"hello")], None);
    r.tx.send(c).expect("send");
    assert!(
        r.published
            .recv_timeout(Duration::from_millis(500))
            .is_err()
    );
    // A later real copy of other content is published.
    let (c, _) = change(&[("UTF8_STRING", b"different")], None);
    r.tx.send(c).expect("send");
    assert!(r.published.recv_timeout(Duration::from_secs(2)).is_ok());
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn newest_inbound_clip_wins_over_a_slow_older_one() {
    let r = rig(true, true, false);
    *r.service.fetch_delay.lock().expect("d") = Duration::from_millis(200);
    r.engine.on_remote_clip(item(
        "old",
        ClipKind::Text,
        &[("text/plain", b"old"), ("text/html", b"<p>old</p>")],
        &r,
    ));
    std::thread::sleep(Duration::from_millis(100));
    r.engine
        .on_remote_clip(item("new", ClipKind::Text, &[("text/plain", b"new")], &r));
    wait(|| (r.backend.written.lock().expect("w").len() == 1).then_some(()));
    std::thread::sleep(Duration::from_millis(500));
    let written = r.backend.written.lock().expect("w");
    assert_eq!(written.len(), 1, "the older clip was dropped");
    assert_eq!(
        written[0].find("UTF8_STRING").map(|x| x.data.clone()),
        Some(b"new".to_vec())
    );
    drop(written);
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn an_x11_oversize_write_is_skipped_with_a_status_not_held() {
    let r = rig(true, true, true);
    r.engine.on_remote_clip(item(
        "a",
        ClipKind::Text,
        &[("text/plain", &vec![b'z'; 5000])],
        &r,
    ));
    let s = wait(|| Some(r.engine.status()).filter(|s| s.last_event.contains("X11 cannot take")));
    assert!(s.pending.is_none());
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn shutdown_is_prompt_stops_the_backend_once_and_refuses_new_work() {
    let r = rig(true, true, false);
    *r.service.fetch_delay.lock().expect("d") = Duration::from_millis(300);
    r.engine.on_remote_clip(item(
        "a",
        ClipKind::Text,
        &[("text/plain", b"x"), ("text/html", b"y")],
        &r,
    ));
    std::thread::sleep(Duration::from_millis(50));
    let t = std::time::Instant::now();
    r.engine.shutdown(Duration::from_secs(1));
    assert!(
        t.elapsed() < Duration::from_millis(1500),
        "{:?}",
        t.elapsed()
    );
    assert_eq!(r.backend.shut.load(Ordering::SeqCst), 1);
    r.engine
        .on_remote_clip(item("b", ClipKind::Text, &[("text/plain", b"late")], &r));
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        r.backend.written.lock().expect("w").is_empty(),
        "nothing is written after shutdown"
    );
}

#[test]
fn files_clips_are_reported_until_the_files_handler_exists() {
    let r = rig(true, true, false);
    r.engine.on_remote_clip(item("f", ClipKind::Files, &[], &r));
    wait(|| r.engine.status().last_event.contains("files").then_some(()));
    r.engine.shutdown(Duration::from_secs(1));
}
