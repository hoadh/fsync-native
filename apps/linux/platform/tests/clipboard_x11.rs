//! X11 backend against a private Xvfb (the real X server is never involved).
mod common;

use common::{Xvfb, need, next_change, text_entry, wait_for};
use fsync_linux_platform::clipboard::x11_backend::X11Backend;
use fsync_linux_platform::clipboard::{Backend, KDE_HINT_MIME, MARKER_MIME, WriteError};
use std::io::Write;
use std::process::Stdio;
use std::time::Duration;

fn xclip_in(x: &Xvfb, target: &str, data: &[u8]) {
    let mut c = x
        .cmd("xclip")
        .args(["-selection", "clipboard", "-t", target, "-i"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("xclip");
    c.stdin
        .take()
        .expect("stdin")
        .write_all(data)
        .expect("write");
    // xclip forks into the background once it owns the selection; the parent exits.
    c.wait().expect("xclip exit");
}

/// Foreground xclip (`-quiet`): it does not fork after connecting, so the PID the X server saw
/// at connect time is the process that keeps serving (xclip's default fork would report the
/// already exited parent, which no real clipboard owner does).
fn xclip_serving(x: &Xvfb, target: &str, data: &[u8]) -> std::process::Child {
    let mut c = x
        .cmd("xclip")
        .args(["-quiet", "-selection", "clipboard", "-t", target, "-i"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("xclip");
    c.stdin
        .take()
        .expect("stdin")
        .write_all(data)
        .expect("write");
    c
}

fn xclip_out(x: &Xvfb, target: &str) -> Vec<u8> {
    x.cmd("xclip")
        .args(["-selection", "clipboard", "-o", "-t", target])
        .output()
        .expect("xclip -o")
        .stdout
}

#[test]
fn watch_read_incr_source_write_and_lost() {
    if !need(&["Xvfb", "xclip"]) {
        return;
    }
    let x = Xvfb::start();
    let backend = X11Backend::start(Some(x.display.clone())).expect("backend");
    let rx = backend.take_changes().expect("changes");

    let mut owner = xclip_serving(&x, "UTF8_STRING", "zażółć gęślą".as_bytes());
    let change = next_change(&rx);
    assert!(
        change.mimes.iter().any(|m| m == "UTF8_STRING"),
        "{:?}",
        change.mimes
    );
    let got = change
        .reader
        .read("UTF8_STRING", 1024, Duration::from_secs(2))
        .expect("read");
    assert_eq!(got, "zażółć gęślą".as_bytes());
    // The copying process is identified (XRes client PID -> /proc comm).
    let src = change.source.as_ref().expect("source app");
    assert_eq!(src.name.as_deref(), Some("xclip"), "{src:?}");
    assert_eq!(src.pid, Some(owner.id()));
    let _ = owner.kill();
    let _ = owner.wait();

    // INCR receive: a copy far beyond one property is read completely, and the cap holds.
    let big: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
    xclip_in(&x, "image/png", &big);
    let incr = next_change(&rx);
    let all = incr
        .reader
        .read("image/png", 8 << 20, Duration::from_secs(5))
        .expect("incr read");
    assert!(
        all == big,
        "INCR content differs ({} vs {} bytes)",
        all.len(),
        big.len()
    );
    assert_eq!(
        incr.reader
            .read("image/png", 1 << 20, Duration::from_secs(5)),
        Err(fsync_linux_platform::clipboard::ReadError::TooLarge)
    );

    // Write: pasteable under the aliases, marker and hint advertised, own event has the marker.
    backend.write(text_entry("from fsync")).expect("write");
    assert!(backend.owned());
    assert_eq!(xclip_out(&x, "UTF8_STRING"), b"from fsync");
    assert_eq!(xclip_out(&x, "STRING"), b"from fsync");
    let targets = String::from_utf8(xclip_out(&x, "TARGETS")).expect("utf8");
    assert!(targets.lines().any(|l| l == MARKER_MIME), "{targets}");
    assert!(targets.lines().any(|l| l == KDE_HINT_MIME), "{targets}");
    assert_eq!(xclip_out(&x, KDE_HINT_MIME), b"secret");

    // STRING is Latin-1: text outside it is simply not offered under STRING.
    backend
        .write(text_entry("zażółć"))
        .expect("write non-latin1");
    assert_eq!(xclip_out(&x, "UTF8_STRING"), "zażółć".as_bytes());
    let targets = String::from_utf8(xclip_out(&x, "TARGETS")).expect("utf8");
    assert!(!targets.lines().any(|l| l == "STRING"), "{targets}");

    // Another client taking the selection ends our ownership.
    xclip_in(&x, "UTF8_STRING", b"someone else");
    wait_for(3, "ownership lost", || (!backend.owned()).then_some(()));

    // No INCR on the sending side: an entry over the server's request limit is refused.
    let too_big = text_entry(&"y".repeat(40 << 20));
    assert!(matches!(
        backend.write(too_big),
        Err(WriteError::TooLargeForX11(_))
    ));

    let t = std::time::Instant::now();
    backend.shutdown();
    assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
    assert_eq!(backend.write(text_entry("late")), Err(WriteError::Closed));
}

#[test]
fn own_write_is_skipped_and_hint_is_visible_by_name() {
    if !need(&["Xvfb", "xclip"]) {
        return;
    }
    let x = Xvfb::start();
    let backend = X11Backend::start(Some(x.display.clone())).expect("backend");
    let rx = backend.take_changes().expect("changes");
    backend.write(text_entry("mine")).expect("write");
    assert!(
        rx.recv_timeout(Duration::from_millis(500)).is_err(),
        "our own write is not announced"
    );
    xclip_in(&x, "UTF8_STRING", b"plain");
    assert!(next_change(&rx).mimes.iter().any(|m| m == "UTF8_STRING"));
    backend.shutdown();
}
