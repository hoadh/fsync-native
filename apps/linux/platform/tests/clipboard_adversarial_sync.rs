//! Adversarial pass, part 2 (QA, 2026-11-06): the real sync engine (lanes, cap budget, newest
//! wins, pause, uri-list rules) over a fake backend and fake core service. The real Wayland and
//! X11 backends are in `clipboard_adversarial_real.rs`.
mod common;

use common::clip_fakes::{Home, MemBackend, Svc, change, denylist_for, home_with_secrets};
use fsync_core::service::LocalClip;
use fsync_linux_platform::clipboard::{Backend, ClipChange};
use fsync_linux_platform::sync::{SyncEngine, SyncEnv, SyncSettings, SyncStatus};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Rig {
    engine: Arc<SyncEngine>,
    tx: Sender<ClipChange>,
    published: Receiver<LocalClip>,
    home: Home,
}

fn rig(max_clip_bytes: u64) -> Rig {
    let home = home_with_secrets();
    let (tx, rx) = mpsc::channel();
    let (ptx, prx) = mpsc::channel();
    let backend: Arc<dyn Backend> = Arc::new(MemBackend {
        rx: Mutex::new(Some(rx)),
        written: Mutex::new(vec![]),
    });
    let svc = Arc::new(Svc {
        published: ptx,
        reps: Mutex::new(HashMap::new()),
        settings: Mutex::new(SyncSettings {
            active: true,
            auto: true,
            max_clip_bytes,
            denylist: vec![],
        }),
    });
    let env = SyncEnv {
        paths: denylist_for(&home),
        files: None,
    };
    let engine = SyncEngine::start(
        svc.clone(),
        Some(backend),
        env,
        SyncStatus::default(),
        Arc::new(|| {}),
    )
    .expect("engine");
    Rig {
        engine,
        tx,
        published: prx,
        home,
    }
}

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

#[test]
fn size_cap_is_inclusive_and_one_byte_over_is_skipped_with_status() {
    let r = rig(10_000);
    r.tx.send(change(&[("text/plain", &vec![b'a'; 10_000])]))
        .unwrap();
    let clip = r
        .published
        .recv_timeout(Duration::from_secs(2))
        .expect("exactly the cap is published");
    assert_eq!(clip.reps[0].data.len(), 10_000);
    r.tx.send(change(&[("text/plain", &vec![b'a'; 10_001])]))
        .unwrap();
    assert!(
        r.published
            .recv_timeout(Duration::from_millis(800))
            .is_err()
    );
    let s = wait_status(&r.engine, "larger than the clip size limit");
    assert!(s.contains("larger than the clip size limit"), "{s}");
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn size_cap_is_one_budget_over_all_reps_of_a_clip() {
    let r = rig(10_000);
    // Text is read first (4 000), so the html may use the remaining 6 000 exactly.
    r.tx.send(change(&[
        ("text/html", &vec![b'<'; 6_000]),
        ("text/plain", &vec![b'x'; 4_000]),
    ]))
    .unwrap();
    let ok = r
        .published
        .recv_timeout(Duration::from_secs(2))
        .expect("published");
    assert_eq!(ok.reps.len(), 2);
    r.tx.send(change(&[
        ("text/html", &vec![b'<'; 6_000]),
        ("text/plain", &vec![b'x'; 4_001]),
    ]))
    .unwrap();
    assert!(
        r.published
            .recv_timeout(Duration::from_millis(800))
            .is_err()
    );
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn uri_lists_into_protected_paths_are_never_published_but_a_plain_file_is() {
    let r = rig(10_000);
    let home = r.home.home.clone();
    let fm = "application/x-kde-cutselection";
    // `..` into ~/.ssh, a symlink into the data dir, and a remote host: nothing published.
    let link = home.join("Documents/notes.txt");
    std::os::unix::fs::symlink(home.join("data/secret.db"), &link).expect("link");
    for list in [
        format!("file://{}/Documents/../.ssh/id_ed25519\r\n", home.display()),
        format!("file://{}\n", link.display()),
        format!("file://remote.example/{}/Documents/a.txt\n", home.display()),
    ] {
        r.tx.send(change(&[("text/uri-list", list.as_bytes()), (fm, b"")]))
            .unwrap();
        assert!(
            r.published
                .recv_timeout(Duration::from_millis(600))
                .is_err(),
            "published a refused list: {list}"
        );
    }
    // Control: the same mechanism publishes a plain file, canonicalized.
    let ok = format!("file://{}/Documents/a.txt\r\n", home.display());
    r.tx.send(change(&[("text/uri-list", ok.as_bytes()), (fm, b"")]))
        .unwrap();
    let clip = r
        .published
        .recv_timeout(Duration::from_secs(2))
        .expect("file clip");
    assert!(clip.reps.is_empty());
    assert_eq!(
        clip.files,
        [home
            .join("Documents/a.txt")
            .canonicalize()
            .unwrap()
            .to_string_lossy()]
    );
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn ten_copies_in_200_ms_publish_the_newest_with_no_backlog() {
    let r = rig(10_000);
    for i in 0..10 {
        r.tx.send(change(&[("text/plain", format!("copy-{i}").as_bytes())]))
            .unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }
    let sent_last = Instant::now();
    let mut seen: Vec<String> = Vec::new();
    while let Ok(clip) = r.published.recv_timeout(Duration::from_millis(800)) {
        seen.push(String::from_utf8_lossy(&clip.reps[0].data).into_owned());
        if seen.last().map(String::as_str) == Some("copy-9") {
            assert!(sent_last.elapsed() < Duration::from_millis(900));
        }
    }
    assert_eq!(seen.last().map(String::as_str), Some("copy-9"), "{seen:?}");
    // Published in order, never an older clip after the newest one, and at most one per copy.
    let idx: Vec<usize> = seen
        .iter()
        .map(|s| s.trim_start_matches("copy-").parse().unwrap())
        .collect();
    assert!(idx.windows(2).all(|w| w[0] < w[1]), "{seen:?}");
    assert!(idx.len() <= 10, "{seen:?}");
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn pause_stops_outbound_and_resume_restores_it() {
    let r = rig(10_000);
    r.engine.set_paused(true);
    r.tx.send(change(&[("text/plain", b"while paused")]))
        .unwrap();
    assert!(
        r.published
            .recv_timeout(Duration::from_millis(600))
            .is_err()
    );
    r.engine.set_paused(false);
    r.tx.send(change(&[("text/plain", b"after resume")]))
        .unwrap();
    let clip = r
        .published
        .recv_timeout(Duration::from_secs(2))
        .expect("resumed");
    assert_eq!(clip.reps[0].data, b"after resume");
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn gap_engine_must_not_publish_a_hinted_copy_in_upper_case() {
    let r = rig(10_000);
    r.tx.send(change(&[
        ("text/plain", b"hunter2"),
        ("X-KDE-PASSWORDMANAGERHINT", b"secret"),
    ]))
    .unwrap();
    assert!(
        r.published
            .recv_timeout(Duration::from_millis(800))
            .is_err()
    );
    r.engine.shutdown(Duration::from_secs(1));
}

#[test]
fn gap_engine_must_publish_text_offered_as_charset_upper_case() {
    let r = rig(10_000);
    r.tx.send(change(&[("text/plain;charset=UTF-8", b"hello")]))
        .unwrap();
    assert!(r.published.recv_timeout(Duration::from_secs(2)).is_ok());
    r.engine.shutdown(Duration::from_secs(1));
}
