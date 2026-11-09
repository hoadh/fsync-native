//! Wayland backend against a private headless sway (the real clipboard is never involved).
mod common;

use common::{Sway, need, next_change, text_entry, wait_for};
use fsync_linux_platform::clipboard::wayland_backend::WaylandBackend;
use fsync_linux_platform::clipboard::{
    Backend, KDE_HINT_MIME, MARKER_MIME, Protocol, ReadError, WriteError,
};
use std::io::Write;
use std::process::Stdio;
use std::time::{Duration, Instant};

fn copy(sway: &Sway, mime: &str, data: &[u8], extra: &[&str]) {
    let mut c = sway
        .cmd("wl-copy")
        .args(["-t", mime])
        .args(extra)
        .stdin(Stdio::piped())
        .spawn()
        .expect("wl-copy");
    c.stdin
        .take()
        .expect("stdin")
        .write_all(data)
        .expect("write");
    c.wait().expect("wl-copy exit");
}

#[test]
fn watch_read_write_cancel_and_stalled_paste() {
    if !need(&["sway", "wl-copy", "wl-paste"]) {
        return;
    }
    let sway = Sway::start();
    let backend = WaylandBackend::start_on(sway.connection()).expect("backend");
    let rx = backend.take_changes().expect("changes");
    assert!(backend.take_changes().is_none(), "changes are taken once");

    // Watch: the MIME list arrives with the change, bytes only on demand.
    copy(&sway, "text/plain", b"hello wayland", &[]);
    let change = next_change(&rx);
    assert!(
        change.mimes.iter().any(|m| m == "text/plain"),
        "{:?}",
        change.mimes
    );
    let got = change
        .reader
        .read("text/plain", 1024, Duration::from_secs(2))
        .expect("read");
    assert_eq!(got, b"hello wayland");
    assert_eq!(
        change.reader.read("text/plain", 3, Duration::from_secs(2)),
        Err(ReadError::TooLarge)
    );

    // A hinted copy shows its hint in the MIME list (no bytes needed to classify it).
    copy(&sway, "text/plain", b"s3cret", &["--sensitive"]);
    let hinted = next_change(&rx);
    assert!(
        hinted.mimes.iter().any(|m| m == KDE_HINT_MIME),
        "{:?}",
        hinted.mimes
    );
    // The older change can no longer be read: it was replaced.
    assert_eq!(
        change
            .reader
            .read("text/plain", 1024, Duration::from_secs(1)),
        Err(ReadError::Stale)
    );

    // Write: our entry is pasteable under every alias and carries the marker.
    backend.write(text_entry("from fsync")).expect("write");
    assert!(backend.owned());
    for alias in ["UTF8_STRING", "text/plain;charset=utf-8", "text/plain"] {
        let out = sway
            .cmd("wl-paste")
            .args(["--no-newline", "-t", alias])
            .output()
            .expect("wl-paste");
        assert_eq!(out.stdout, b"from fsync", "{alias}");
    }
    let types = String::from_utf8(
        sway.cmd("wl-paste")
            .arg("-l")
            .output()
            .expect("wl-paste -l")
            .stdout,
    )
    .expect("utf8");
    assert!(types.lines().any(|l| l == MARKER_MIME), "{types}");
    let own = next_change(&rx);
    assert!(
        own.mimes.iter().any(|m| m == MARKER_MIME),
        "our own write is recognisable by name"
    );

    // Another client taking the selection ends our ownership (cancelled is normal).
    copy(&sway, "text/plain", b"someone else", &[]);
    wait_for(3, "ownership lost", || (!backend.owned()).then_some(()));

    // A paste that never reads must not block replacing the entry or shutting down.
    backend
        .write(text_entry(&"x".repeat(4 << 20)))
        .expect("big write");
    let mut stalled = sway
        .cmd("wl-paste")
        .args(["--no-newline", "-t", "text/plain"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("stalled paste");
    std::thread::sleep(Duration::from_millis(400));
    backend
        .write(text_entry("small again"))
        .expect("replace while a paste stalls");
    let out = sway
        .cmd("wl-paste")
        .args(["--no-newline", "-t", "text/plain"])
        .output()
        .expect("wl-paste");
    assert_eq!(out.stdout, b"small again");
    let t = Instant::now();
    backend.shutdown();
    assert!(
        t.elapsed() < Duration::from_secs(1),
        "shutdown took {:?}",
        t.elapsed()
    );
    assert_eq!(backend.write(text_entry("late")), Err(WriteError::Closed));
    let _ = stalled.kill();
    let _ = stalled.wait();
}

#[test]
fn no_change_is_announced_for_the_selection_that_exists_at_start() {
    if !need(&["sway", "wl-copy"]) {
        return;
    }
    let sway = Sway::start();
    copy(&sway, "text/plain", b"already there", &[]);
    let backend = WaylandBackend::start_on(sway.connection()).expect("backend");
    let rx = backend.take_changes().expect("changes");
    assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
    backend.shutdown();
}

#[test]
fn the_zwlr_fallback_watches_and_serves_too() {
    if !need(&["sway", "wl-copy", "wl-paste"]) {
        return;
    }
    let sway = Sway::start();
    let backend =
        WaylandBackend::start_with(sway.connection(), Some(Protocol::Wlr)).expect("zwlr backend");
    let rx = backend.take_changes().expect("changes");
    copy(&sway, "text/plain", b"via zwlr", &[]);
    let change = next_change(&rx);
    assert_eq!(
        change
            .reader
            .read("text/plain", 100, Duration::from_secs(2))
            .expect("read"),
        b"via zwlr"
    );
    backend.write(text_entry("served by zwlr")).expect("write");
    let out = sway
        .cmd("wl-paste")
        .args(["--no-newline", "-t", "UTF8_STRING"])
        .output()
        .expect("wl-paste");
    assert_eq!(out.stdout, b"served by zwlr");
    backend.shutdown();
}
