use super::tests_rig::*;
use crate::clipboard::{KDE_HINT_MIME, MARKER_MIME, SourceApp};
use std::sync::atomic::Ordering;
use std::time::Duration;

#[test]
fn text_and_rich_changes_are_published_in_wire_order() {
    let r = rig(true, true, false);
    let (c, _) = change(
        &[
            ("text/html", b"<b>x</b>"),
            ("UTF8_STRING", b"x"),
            ("TARGETS", b""),
        ],
        None,
    );
    r.tx.send(c).expect("send");
    let clip = r
        .published
        .recv_timeout(Duration::from_secs(2))
        .expect("published");
    let names: Vec<_> = clip.reps.iter().map(|x| x.mime.as_str()).collect();
    assert_eq!(names, ["text/html", "text/plain"]);
    assert!(clip.files.is_empty());
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn hinted_and_denied_changes_are_never_read_or_published() {
    let r = rig(true, true, false);
    let (hinted, hr) = change(&[("text/plain", b"pw"), (KDE_HINT_MIME, b"secret")], None);
    let denied_src = SourceApp {
        name: Some("KeePassXC".into()),
        ..Default::default()
    };
    let (denied, dr) = change(&[("text/plain", b"pw2")], Some(denied_src));
    r.tx.send(hinted).expect("send");
    std::thread::sleep(Duration::from_millis(150));
    r.tx.send(denied).expect("send");
    assert!(
        r.published
            .recv_timeout(Duration::from_millis(500))
            .is_err()
    );
    assert_eq!(
        hr.reads.load(Ordering::SeqCst) + dr.reads.load(Ordering::SeqCst),
        0,
        "skip before read"
    );
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn own_marker_is_ignored_without_reading() {
    let r = rig(true, true, false);
    let (c, reader) = change(&[("text/plain", b"mine"), (MARKER_MIME, b"1")], None);
    r.tx.send(c).expect("send");
    assert!(
        r.published
            .recv_timeout(Duration::from_millis(400))
            .is_err()
    );
    assert_eq!(reader.reads.load(Ordering::SeqCst), 0);
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn oversize_content_is_skipped_with_a_status() {
    let r = rig(true, true, false);
    let big = vec![b'a'; 20_000];
    let (c, _) = change(&[("text/plain", &big)], None);
    r.tx.send(c).expect("send");
    let s = wait(|| Some(r.engine.status()).filter(|s| s.last_event.contains("size limit")));
    assert!(s.last_event.starts_with("Not sent"));
    assert!(r.published.try_recv().is_err());
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn uri_list_needs_a_file_manager_type_and_respects_the_path_denylist() {
    let r = rig(true, true, false);
    let ok = r.home.join("Documents/a.txt");
    std::fs::write(&ok, b"x").expect("write");
    let key = r.home.join(".ssh/id_ed25519");
    std::fs::write(&key, b"k").expect("write");
    let uri = |p: &std::path::Path| format!("file://{}\r\n", p.display()).into_bytes();

    // Bare uri-list + text: files are ignored, the text still syncs.
    let (bare, _) = change(
        &[("text/uri-list", &uri(&ok)), ("text/plain", b"names")],
        None,
    );
    r.tx.send(bare).expect("send");
    let clip = r
        .published
        .recv_timeout(Duration::from_secs(2))
        .expect("text clip");
    assert!(clip.files.is_empty() && clip.reps.len() == 1);

    // File-manager copy of a protected file: nothing is published.
    let (secret, _) = change(
        &[
            ("text/uri-list", &uri(&key)),
            ("x-special/gnome-copied-files", b"copy\n"),
        ],
        None,
    );
    r.tx.send(secret).expect("send");
    assert!(
        r.published
            .recv_timeout(Duration::from_millis(500))
            .is_err()
    );

    // File-manager copy of an ordinary file becomes a files clip.
    let (fm, _) = change(
        &[
            ("text/uri-list", &uri(&ok)),
            ("application/x-kde-cutselection", b"0"),
        ],
        None,
    );
    r.tx.send(fm).expect("send");
    let clip = r
        .published
        .recv_timeout(Duration::from_secs(2))
        .expect("files clip");
    assert_eq!(clip.files.len(), 1);
    assert!(clip.files[0].ends_with("Documents/a.txt") && clip.reps.is_empty());
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn shutdown_waits_for_a_publish_that_is_already_running() {
    let r = rig(true, true, false);
    *r.service.publish_delay.lock().expect("d") = Duration::from_millis(300);
    let (c, _) = change(&[("text/plain", b"slow")], None);
    r.tx.send(c).expect("send");
    std::thread::sleep(Duration::from_millis(100)); // the job is inside publish now
    r.engine.shutdown(Duration::from_secs(1));
    assert!(
        r.published.try_recv().is_ok(),
        "shutdown returned before the running publish finished"
    );
}

#[test]
fn a_slow_publish_outcome_does_not_overwrite_a_newer_event() {
    let r = rig(true, true, false);
    *r.service.publish_delay.lock().expect("d") = Duration::from_millis(500);
    let (c, _) = change(&[("text/plain", b"slow")], None);
    r.tx.send(c).expect("send");
    // The publish is now sleeping inside the service; a received clip reports meanwhile.
    std::thread::sleep(Duration::from_millis(200));
    r.engine.event("Large file clip arrived.");
    r.published
        .recv_timeout(Duration::from_secs(3))
        .expect("published");
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(r.engine.status().last_event, "Large file clip arrived.");
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn a_publish_outcome_is_shown_when_nothing_newer_happened() {
    let r = rig(true, true, false);
    let (c, _) = change(&[("text/plain", b"quick")], None);
    r.tx.send(c).expect("send");
    r.published
        .recv_timeout(Duration::from_secs(2))
        .expect("published");
    wait(|| {
        r.engine
            .status()
            .last_event
            .starts_with("Sent")
            .then_some(())
    });
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn a_slow_publish_outcome_does_not_overwrite_a_received_clip_status() {
    // The inbound lane writes "Received ..." straight into the status (not through `event`).
    let r = rig(true, true, false);
    *r.service.publish_delay.lock().expect("d") = Duration::from_millis(600);
    let (c, _) = change(&[("text/plain", b"slow")], None);
    r.tx.send(c).expect("send");
    std::thread::sleep(Duration::from_millis(200));
    r.engine.on_remote_clip(item(
        "in",
        fsync_proto::ClipKind::Text,
        &[("text/plain", b"hello")],
        &r,
    ));
    wait(|| {
        r.engine
            .status()
            .last_event
            .starts_with("Received")
            .then_some(())
    });
    r.published
        .recv_timeout(Duration::from_secs(3))
        .expect("published");
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        r.engine.status().last_event.starts_with("Received"),
        "got {:?}",
        r.engine.status().last_event
    );
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn a_repeated_identical_received_message_still_counts_as_newer_than_a_slow_publish() {
    let r = rig(true, true, false);
    *r.service.publish_delay.lock().expect("d") = Duration::from_millis(900);
    let (c, _) = change(&[("text/plain", b"slow")], None);
    r.tx.send(c).expect("send");
    std::thread::sleep(Duration::from_millis(200));
    let first = item(
        "in1",
        fsync_proto::ClipKind::Text,
        &[("text/plain", b"one")],
        &r,
    );
    r.engine.on_remote_clip(first);
    wait(|| (r.backend.written.lock().expect("w").len() == 1).then_some(()));
    let before = r.engine.status().last_event;
    // Same sentence again ("Received text from phone."): the text does not change.
    let second = item(
        "in2",
        fsync_proto::ClipKind::Text,
        &[("text/plain", b"two")],
        &r,
    );
    r.engine.on_remote_clip(second);
    wait(|| (r.backend.written.lock().expect("w").len() == 2).then_some(()));
    assert_eq!(r.engine.status().last_event, before);
    r.published
        .recv_timeout(Duration::from_secs(3))
        .expect("published");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        r.engine.status().last_event,
        before,
        "publish outcome overwrote it"
    );
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn status_line_does_not_call_a_pending_device_a_failure() {
    use super::outbound::describe;
    use fsync_core::service::Published;
    let p = |delivered: &[&str], failed: &[&str], pending: &[&str]| Published {
        item_id: "i".into(),
        echo: false,
        delivered: delivered.iter().map(|s| s.to_string()).collect(),
        failed: failed.iter().map(|s| s.to_string()).collect(),
        pending: pending.iter().map(|s| s.to_string()).collect(),
        refused: vec![],
    };
    let text = |x: Published| describe(&x).expect("not an echo");
    assert_eq!(text(p(&["a"], &[], &["b"])), "Sent to 1 device.");
    assert_eq!(text(p(&[], &[], &["a", "b"])), "Sending to 2 devices...");
    assert_eq!(text(p(&[], &["a"], &["b"])), "Sending to 1 device...");
    assert_eq!(
        text(p(&[], &["a"], &[])),
        "Not sent: no other device is reachable."
    );
}
