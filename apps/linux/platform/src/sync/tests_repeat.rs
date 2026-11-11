//! One copy seen twice (an app that sets the selection twice, a few ms apart) is published once;
//! other content, an explicit send, a received clip and a later copy still publish.

use super::shared::{AUTO_REPEAT_WINDOW, admit_auto};
use super::tests_rig::*;
use fsync_proto::ClipKind;
use std::time::{Duration, Instant};

/// A rig whose repeat window is far wider than any runner delay: "within the window" holds.
fn wide_rig() -> Rig {
    let r = rig(true, true, false);
    r.engine.set_repeat_window(Duration::from_secs(60));
    r
}

fn copy(r: &Rig, text: &[u8]) {
    let (c, _) = change(&[("text/plain", text)], None);
    r.tx.send(c).expect("send");
}

/// Waits generously for `n` publishes (loaded CI), then briefly for an unexpected extra one.
fn expect_published(r: &Rig, n: usize) {
    for i in 0..n {
        assert!(
            r.published.recv_timeout(Duration::from_secs(3)).is_ok(),
            "publish {} of {n} missing",
            i + 1
        );
    }
    assert!(
        r.published
            .recv_timeout(Duration::from_millis(250))
            .is_err(),
        "more than {n} publish(es)"
    );
}

#[test]
fn a_cancelled_job_neither_sends_nor_records() {
    let (now, w) = (Instant::now(), AUTO_REPEAT_WINDOW);
    let ms = Duration::from_millis;
    let mut last = None;
    // The first selection's job was replaced before it could send: it must not record, or the
    // second (identical) selection would be dropped and nothing would go out.
    assert!(!admit_auto(&mut last, 7, true, now, w));
    assert!(last.is_none());
    assert!(admit_auto(&mut last, 7, false, now, w));
    assert!(!admit_auto(&mut last, 7, false, now + ms(5), w));
    assert!(admit_auto(&mut last, 8, false, now + ms(6), w));
    assert!(admit_auto(&mut last, 8, false, now + w * 2, w));
}

#[test]
fn the_same_copy_seen_twice_while_the_first_publishes_is_published_once() {
    let r = wide_rig();
    // The first publish is still running when the second selection arrives (the live case).
    *r.service.publish_delay.lock().expect("d") = Duration::from_millis(200);
    copy(&r, b"twice");
    std::thread::sleep(Duration::from_millis(30));
    copy(&r, b"twice");
    expect_published(&r, 1);
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn a_publish_slower_than_the_window_does_not_let_the_second_sighting_through() {
    // Live 2026-11-12: the first publish waited 1.5 s for confirmations while the second
    // sighting (2 ms later) sat in the lane; measured from the publish it was "outside" the 1 s
    // window and went out 1.5 s later. The window is measured between sightings.
    let r = rig(true, true, false);
    r.engine.set_repeat_window(Duration::from_millis(250));
    *r.service.publish_delay.lock().expect("d") = Duration::from_millis(800);
    copy(&r, b"slow");
    std::thread::sleep(Duration::from_millis(20));
    copy(&r, b"slow");
    assert!(r.published.recv_timeout(Duration::from_secs(3)).is_ok());
    // A second publish would need another 800 ms: wait well past it.
    assert!(
        r.published
            .recv_timeout(Duration::from_millis(1500))
            .is_err(),
        "the second sighting was published"
    );
    r.engine.shutdown(Duration::from_secs(2));
}

#[test]
fn the_same_copy_seen_twice_after_the_first_publish_is_published_once() {
    let r = wide_rig();
    copy(&r, b"twice");
    expect_published(&r, 1);
    copy(&r, b"twice");
    expect_published(&r, 0);
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn different_content_and_explicit_sends_are_always_published() {
    let r = wide_rig();
    copy(&r, b"one");
    expect_published(&r, 1);
    copy(&r, b"two");
    expect_published(&r, 1);
    // A copy back to earlier content is new content for the peers.
    copy(&r, b"one");
    expect_published(&r, 1);
    // "Send clipboard now" right after: the user asked for it.
    assert!(r.engine.send_now());
    expect_published(&r, 1);
    // The explicit send reset the record: the same copy again is sent.
    copy(&r, b"one");
    expect_published(&r, 1);
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn a_received_clip_resets_the_record() {
    let r = wide_rig();
    copy(&r, b"mine");
    expect_published(&r, 1);
    // A peer's clip is written to the clipboard: peers now hold it, not "mine".
    r.engine
        .on_remote_clip(item("p", ClipKind::Text, &[("text/plain", b"theirs")], &r));
    wait(|| (!r.backend.written.lock().expect("w").is_empty()).then_some(()));
    copy(&r, b"mine");
    expect_published(&r, 1);
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn the_same_content_copied_again_after_the_window_is_published() {
    let r = rig(true, true, false);
    r.engine.set_repeat_window(Duration::from_millis(50));
    copy(&r, b"again");
    expect_published(&r, 1);
    std::thread::sleep(Duration::from_millis(100));
    copy(&r, b"again");
    expect_published(&r, 1);
    r.engine.shutdown(Duration::from_secs(1));
}
