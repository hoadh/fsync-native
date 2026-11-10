use super::tests_rig::*;
use super::*;
use fsync_proto::ClipKind;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[test]
fn manual_mode_holds_pending_until_paste_and_never_publishes_on_its_own() {
    let r = rig(true, false, false);
    r.engine
        .on_remote_clip(item("a", ClipKind::Text, &[("text/plain", b"held")], &r));
    wait(|| r.engine.status().pending.map(|_| ()));
    assert!(r.backend.written.lock().expect("w").is_empty());
    assert_eq!(r.engine.status().pending.expect("pending").kind, "text");
    // Local copies are held too: no publish without send_now.
    let (c, _) = change(&[("text/plain", b"local")], None);
    r.tx.send(c).expect("send");
    assert!(
        r.published
            .recv_timeout(Duration::from_millis(400))
            .is_err()
    );
    assert_eq!(r.engine.paste(), Paste::Done);
    assert_eq!(r.backend.written.lock().expect("w").len(), 1);
    assert!(r.engine.status().pending.is_none());
    assert_eq!(r.engine.paste(), Paste::Nothing);
    assert!(r.engine.send_now());
    assert!(r.published.recv_timeout(Duration::from_secs(2)).is_ok());
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn without_a_backend_paste_hands_the_entry_to_the_window() {
    let r = rig(false, true, false);
    r.engine.on_remote_clip(item(
        "a",
        ClipKind::Text,
        &[("text/plain", b"for the window")],
        &r,
    ));
    wait(|| r.engine.status().pending.map(|_| ()));
    match r.engine.paste() {
        Paste::NeedsWindow(e) => assert_eq!(
            e.find("text/plain").map(|x| x.data.clone()),
            Some(b"for the window".to_vec())
        ),
        other => panic!("{other:?}"),
    }
    assert!(
        r.engine.status().pending.is_some(),
        "kept until the window confirms"
    );
    r.engine.pasted();
    assert!(r.engine.status().pending.is_none());
    assert!(!r.engine.send_now(), "no change seen yet");
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn pause_stops_both_directions_but_keeps_received_clips_pending() {
    let r = rig(true, true, false);
    r.engine.set_paused(true);
    let (c, _) = change(&[("text/plain", b"x")], None);
    r.tx.send(c).expect("send");
    assert!(
        r.published
            .recv_timeout(Duration::from_millis(400))
            .is_err()
    );
    r.engine
        .on_remote_clip(item("a", ClipKind::Text, &[("text/plain", b"in")], &r));
    wait(|| r.engine.status().pending.map(|_| ()));
    assert!(r.backend.written.lock().expect("w").is_empty());
    r.engine.set_paused(false);
    assert_eq!(r.engine.paste(), Paste::Done);
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn while_signed_out_nothing_is_read_and_received_clips_are_dropped() {
    let r = rig(true, true, false);
    r.service.settings.lock().expect("s").active = false;
    let (c, reader) = change(&[("text/plain", b"x")], None);
    r.tx.send(c).expect("send");
    r.engine
        .on_remote_clip(item("a", ClipKind::Text, &[("text/plain", b"in")], &r));
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(reader.reads.load(Ordering::SeqCst), 0);
    assert!(r.published.try_recv().is_err());
    assert!(r.backend.written.lock().expect("w").is_empty());
    assert!(r.engine.status().pending.is_none());
    assert!(!r.engine.send_now(), "nothing was kept either");
    r.engine.shutdown(Duration::from_secs(1));
}
