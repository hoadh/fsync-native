//! One copy seen twice (an app that sets the selection twice, a few ms apart) is published once;
//! other content, an explicit send and a later copy of the same content still publish.

use super::shared::AUTO_REPEAT_WINDOW;
use super::tests_rig::*;
use std::time::Duration;

fn copy(r: &Rig, text: &[u8]) {
    let (c, _) = change(&[("text/plain", text)], None);
    r.tx.send(c).expect("send");
}

fn published(r: &Rig, within: Duration) -> usize {
    let mut n = 0;
    while r.published.recv_timeout(within).is_ok() {
        n += 1;
    }
    n
}

#[test]
fn the_same_copy_seen_twice_while_the_first_publishes_is_published_once() {
    let r = rig(true, true, false);
    // The first publish is still running when the second selection arrives (the live case).
    *r.service.publish_delay.lock().expect("d") = Duration::from_millis(200);
    copy(&r, b"twice");
    std::thread::sleep(Duration::from_millis(30));
    copy(&r, b"twice");
    assert_eq!(published(&r, Duration::from_millis(600)), 1);
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn the_same_copy_seen_twice_after_the_first_publish_is_published_once() {
    let r = rig(true, true, false);
    copy(&r, b"twice");
    assert_eq!(published(&r, Duration::from_millis(300)), 1);
    copy(&r, b"twice");
    assert_eq!(published(&r, Duration::from_millis(300)), 0);
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn different_content_and_explicit_sends_are_always_published() {
    let r = rig(true, true, false);
    copy(&r, b"one");
    assert_eq!(published(&r, Duration::from_millis(300)), 1);
    copy(&r, b"two");
    assert_eq!(published(&r, Duration::from_millis(300)), 1);
    // A copy back to earlier content is new content for the peers.
    copy(&r, b"one");
    assert_eq!(published(&r, Duration::from_millis(300)), 1);
    // "Send clipboard now" right after: the user asked for it.
    assert!(r.engine.send_now());
    assert_eq!(published(&r, Duration::from_millis(300)), 1);
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn the_same_content_copied_again_after_the_window_is_published() {
    let r = rig(true, true, false);
    copy(&r, b"again");
    assert_eq!(published(&r, Duration::from_millis(300)), 1);
    std::thread::sleep(AUTO_REPEAT_WINDOW);
    copy(&r, b"again");
    assert_eq!(published(&r, Duration::from_millis(300)), 1);
    r.engine.shutdown(Duration::from_secs(1));
}
