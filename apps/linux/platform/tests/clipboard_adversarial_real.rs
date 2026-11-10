//! Adversarial pass, part 3 (QA, 2026-11-06): the real Wayland (private headless sway) and X11
//! (private Xvfb) backends under the real sync engine. Never the user's session.
mod common;

use common::clip_fakes::Svc;
use common::{Sway, Xvfb, need, next_change, text_entry};
use fsync_core::service::{ClipItem, LocalClip, RepInfo};
use fsync_linux_platform::clipboard::path_denylist::PathDenylist;
use fsync_linux_platform::clipboard::wayland_backend::WaylandBackend;
use fsync_linux_platform::clipboard::x11_backend::X11Backend;
use fsync_linux_platform::clipboard::{Backend, ReadError, WriteError};
use fsync_linux_platform::sync::{SyncEngine, SyncEnv, SyncSettings, SyncStatus};
use fsync_proto::ClipKind;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ---------- real Wayland (private headless sway + wl-clipboard) ----------

/// Polls the engine's status line until it contains `needle` (or 3 s pass).
fn wait_status(engine: &SyncEngine, needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let s = engine.status().last_event;
        if s.contains(needle) || Instant::now() > deadline {
            return s;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wl_copy(sway: &Sway, mime: &str, data: &[u8]) {
    let mut c = sway
        .cmd("wl-copy")
        .args(["-t", mime])
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

fn wl_paste(sway: &Sway) -> Vec<u8> {
    sway.cmd("wl-paste")
        .args(["--no-newline", "-t", "text/plain"])
        .output()
        .expect("wl-paste")
        .stdout
}

fn svc_with(max: u64) -> (Arc<Svc>, Receiver<LocalClip>) {
    let (ptx, prx) = mpsc::channel();
    (
        Arc::new(Svc {
            published: ptx,
            reps: Mutex::new(HashMap::new()),
            settings: Mutex::new(SyncSettings {
                active: true,
                auto: true,
                max_clip_bytes: max,
                denylist: vec![],
            }),
        }),
        prx,
    )
}

fn real_engine(backend: Arc<dyn Backend>, svc: Arc<Svc>) -> Arc<SyncEngine> {
    // Text-only tests: no uri-list reaches the path rules, so non-existent roots are fine.
    let nowhere = Path::new("/nonexistent-qa-home");
    let env = SyncEnv {
        paths: PathDenylist::new(
            nowhere,
            &nowhere.join("data"),
            &nowhere.join(".local/share"),
            &nowhere.join(".config"),
        ),
        files: None,
    };
    SyncEngine::start(
        svc,
        Some(backend),
        env,
        SyncStatus::default(),
        Arc::new(|| {}),
    )
    .expect("engine")
}

#[test]
fn wayland_ten_rapid_copies_publish_the_newest_for_real() {
    if !need(&["sway", "wl-copy", "wl-paste"]) {
        return;
    }
    let sway = Sway::start();
    let backend = WaylandBackend::start_on(sway.connection()).expect("backend");
    let (svc, published) = svc_with(10_000);
    let engine = real_engine(backend, svc);
    for i in 0..10 {
        wl_copy(&sway, "text/plain", format!("copy-{i}").as_bytes());
    }
    let mut seen = Vec::new();
    while let Ok(clip) = published.recv_timeout(Duration::from_millis(1500)) {
        seen.push(String::from_utf8_lossy(&clip.reps[0].data).into_owned());
    }
    eprintln!("wayland rapid copies: published {seen:?}");
    assert_eq!(seen.last().map(String::as_str), Some("copy-9"), "{seen:?}");
    engine.shutdown(Duration::from_secs(1));
}

#[test]
fn wayland_size_cap_boundary_on_the_real_pipe() {
    if !need(&["sway", "wl-copy", "wl-paste"]) {
        return;
    }
    let sway = Sway::start();
    let backend = WaylandBackend::start_on(sway.connection()).expect("backend");
    let (svc, published) = svc_with(20_000);
    let engine = real_engine(backend, svc);
    wl_copy(&sway, "text/plain", &vec![b'a'; 20_000]);
    let clip = published
        .recv_timeout(Duration::from_secs(5))
        .expect("exactly the cap");
    assert_eq!(clip.reps[0].data.len(), 20_000);
    wl_copy(&sway, "text/plain", &vec![b'b'; 20_001]);
    assert!(published.recv_timeout(Duration::from_millis(800)).is_err());
    let s = wait_status(&engine, "larger than the clip size limit");
    assert!(s.contains("larger than the clip size limit"), "{s}");
    engine.shutdown(Duration::from_secs(1));
}

#[test]
fn wayland_received_clip_is_written_and_never_sent_back() {
    if !need(&["sway", "wl-copy", "wl-paste"]) {
        return;
    }
    let sway = Sway::start();
    let backend = WaylandBackend::start_on(sway.connection()).expect("backend");
    let (svc, published) = svc_with(10_000);
    svc.reps.lock().unwrap().insert(
        "remote".into(),
        vec![("text/plain".into(), b"from the peer".to_vec())],
    );
    let engine = real_engine(backend, svc.clone());
    engine.on_remote_clip(ClipItem {
        item_id: "remote".into(),
        from: "node".into(),
        from_name: "phone".into(),
        kind: ClipKind::Text,
        ts_ms: 0,
        preview: None,
        reps: vec![RepInfo {
            mime: "text/plain".into(),
            size: 13,
        }],
        files: vec![],
        total_size: 13,
        push: false,
    });
    // Non-vacuity: the clipboard really holds the received text before we check for echoes.
    let deadline = Instant::now() + Duration::from_secs(5);
    while wl_paste(&sway) != b"from the peer" {
        assert!(
            Instant::now() < deadline,
            "the received clip was never written"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        published.recv_timeout(Duration::from_millis(1500)).is_err(),
        "our own write was sent back"
    );
    engine.shutdown(Duration::from_secs(1));
}

#[test]
fn wayland_stalled_paster_does_not_hold_shutdown_past_one_second() {
    if !need(&["sway", "wl-copy", "wl-paste"]) {
        return;
    }
    let sway = Sway::start();
    let backend = WaylandBackend::start_on(sway.connection()).expect("backend");
    let (svc, _published) = svc_with(50 << 20);
    let engine = real_engine(backend.clone(), svc);
    let entry = text_entry(&"x".repeat(4 << 20));
    backend.write(entry).expect("big write");
    // A paster that takes the offer and never reads it.
    let mut stalled = sway
        .cmd("wl-paste")
        .args(["-t", "text/plain"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("stalled paste");
    std::thread::sleep(Duration::from_millis(400));
    // Non-vacuous: the paster must still be alive and blocked on its unread pipe.
    assert!(
        stalled.try_wait().expect("try_wait").is_none(),
        "paster exited early"
    );
    let t = Instant::now();
    engine.shutdown(Duration::from_secs(1));
    let took = t.elapsed();
    eprintln!("wayland shutdown with a stalled paster took {took:?}");
    let _ = stalled.kill();
    let _ = stalled.wait();
    assert!(took < Duration::from_millis(1000), "shutdown took {took:?}");
}

// ---------- real X11 (private Xvfb + xclip) ----------

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
    c.wait().expect("xclip exit");
}

fn xclip_out(x: &Xvfb, target: &str) -> Vec<u8> {
    x.cmd("xclip")
        .args(["-selection", "clipboard", "-o", "-t", target])
        .output()
        .expect("xclip -o")
        .stdout
}

#[test]
fn x11_incr_utf8_text_over_one_mebibyte_is_read_completely() {
    if !need(&["Xvfb", "xclip"]) {
        return;
    }
    let x = Xvfb::start();
    let backend = X11Backend::start(Some(x.display.clone())).expect("backend");
    let rx = backend.take_changes().expect("changes");
    let text: Vec<u8> = (0..2_500_000u32).map(|i| b'a' + (i % 26) as u8).collect();
    xclip_in(&x, "UTF8_STRING", &text);
    let change = next_change(&rx);
    let got = change
        .reader
        .read("UTF8_STRING", 8 << 20, Duration::from_secs(5))
        .expect("incr read");
    assert!(
        got == text,
        "INCR text differs: {} vs {} bytes",
        got.len(),
        text.len()
    );
    assert_eq!(
        change
            .reader
            .read("UTF8_STRING", 1 << 20, Duration::from_secs(5)),
        Err(ReadError::TooLarge)
    );
    backend.shutdown();
}

#[test]
fn x11_write_above_the_server_request_limit_is_refused_and_small_one_is_kept() {
    if !need(&["Xvfb", "xclip"]) {
        return;
    }
    let x = Xvfb::start();
    let backend = X11Backend::start(Some(x.display.clone())).expect("backend");
    backend
        .write(text_entry(&"y".repeat(1 << 20)))
        .expect("1 MiB fits in one request");
    assert!(backend.owned());
    match backend.write(text_entry(&"y".repeat(20 << 20))) {
        Err(WriteError::TooLargeForX11(n)) => assert!(n >= 20 << 20, "{n}"),
        other => panic!("expected TooLargeForX11, got {other:?}"),
    }
    backend.shutdown();
}

#[test]
fn x11_received_clip_is_written_and_never_sent_back() {
    if !need(&["Xvfb", "xclip"]) {
        return;
    }
    let x = Xvfb::start();
    let backend = X11Backend::start(Some(x.display.clone())).expect("backend");
    let (svc, published) = svc_with(10_000);
    svc.reps.lock().unwrap().insert(
        "remote".into(),
        vec![("text/plain".into(), "from the peer".as_bytes().to_vec())],
    );
    let engine = real_engine(backend, svc);
    engine.on_remote_clip(ClipItem {
        item_id: "remote".into(),
        from: "node".into(),
        from_name: "phone".into(),
        kind: ClipKind::Text,
        ts_ms: 0,
        preview: None,
        reps: vec![RepInfo {
            mime: "text/plain".into(),
            size: 13,
        }],
        files: vec![],
        total_size: 13,
        push: false,
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while xclip_out(&x, "UTF8_STRING") != b"from the peer" {
        assert!(
            Instant::now() < deadline,
            "the received clip was never written"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        published.recv_timeout(Duration::from_millis(1500)).is_err(),
        "our own X11 write was sent back"
    );
    let t = Instant::now();
    engine.shutdown(Duration::from_secs(1));
    eprintln!("x11 quit while owning took {:?}", t.elapsed());
    assert!(t.elapsed() < Duration::from_secs(6));
}

#[test]
fn x11_plain_text_copy_is_read_back_from_its_owner() {
    if !need(&["Xvfb", "xclip"]) {
        return;
    }
    let x = Xvfb::start();
    let backend = X11Backend::start(Some(x.display.clone())).expect("backend");
    let rx = backend.take_changes().expect("changes");
    xclip_in(&x, "UTF8_STRING", b"plain copy");
    let change = next_change(&rx);
    assert_eq!(
        change
            .reader
            .read("UTF8_STRING", 1024, Duration::from_secs(2))
            .expect("read"),
        b"plain copy"
    );
    assert!(change.mimes.iter().any(|m| m == "UTF8_STRING"));
    backend.shutdown();
}
