//! One copy seen twice (an app that sets the selection twice, a few ms apart) is published once;
//! other content, an explicit send, a received clip and a later copy still publish.

use super::shared::{AUTO_REPEAT_WINDOW, admit_auto};
use super::tests_rig::*;
use fsync_proto::ClipKind;
use std::time::{Duration, Instant};

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
    let now = Instant::now();
    let mut last = None;
    // The first selection's job was replaced before it could send: it must not record, or the
    // second (identical) selection would be dropped and nothing would go out.
    assert!(!admit_auto(&mut last, 7, true, now));
    assert!(last.is_none());
    assert!(admit_auto(&mut last, 7, false, now));
    assert!(!admit_auto(
        &mut last,
        7,
        false,
        now + Duration::from_millis(5)
    ));
    assert!(admit_auto(
        &mut last,
        8,
        false,
        now + Duration::from_millis(6)
    ));
    assert!(admit_auto(
        &mut last,
        8,
        false,
        now + AUTO_REPEAT_WINDOW * 2
    ));
}

#[test]
fn the_same_copy_seen_twice_while_the_first_publishes_is_published_once() {
    let r = rig(true, true, false);
    // The first publish is still running when the second selection arrives (the live case).
    *r.service.publish_delay.lock().expect("d") = Duration::from_millis(200);
    copy(&r, b"twice");
    std::thread::sleep(Duration::from_millis(30));
    copy(&r, b"twice");
    expect_published(&r, 1);
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn the_same_copy_seen_twice_after_the_first_publish_is_published_once() {
    let r = rig(true, true, false);
    copy(&r, b"twice");
    expect_published(&r, 1);
    copy(&r, b"twice");
    expect_published(&r, 0);
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn different_content_and_explicit_sends_are_always_published() {
    let r = rig(true, true, false);
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
    let r = rig(true, true, false);
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
    copy(&r, b"again");
    expect_published(&r, 1);
    std::thread::sleep(AUTO_REPEAT_WINDOW);
    copy(&r, b"again");
    expect_published(&r, 1);
    r.engine.shutdown(Duration::from_secs(1));
}
