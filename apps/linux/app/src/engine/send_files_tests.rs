use super::super::send_stage::{MAX_INFLIGHT, stage_and_show, stage_with};
use super::*;
use crate::core_state::ui_state::EngineState;
use fsync_linux_platform::clipboard::path_denylist::PathDenylist;
use fsync_linux_platform::config_file::AppConfig;
use fsync_linux_platform::ipc::MAX_PATHS;
use fsync_linux_platform::paths::Paths;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

fn core() -> Arc<Core> {
    let dir = PathBuf::from("/nonexistent/fsync-send-test");
    let paths = Paths {
        data_dir: dir.clone(),
        config_file: dir.join("config.toml"),
        autostart_dir: dir.join("autostart"),
        runtime_dir: dir.join("run"),
    };
    let cfg = AppConfig {
        api: "http://127.0.0.1:9".into(),
        relays: Vec::new(),
        device_name: "t".into(),
        allow_insecure_http: true,
    };
    Arc::new(Core::new(paths, dir, cfg, None))
}

fn deny_for(home: &Path) -> PathDenylist {
    PathDenylist::new(
        home,
        &home.join(".local/share/fsync"),
        &home.join(".local/share"),
        &home.join(".config"),
    )
}

#[test]
fn home_files_pass_and_protected_or_missing_are_refused() {
    let t = tempfile::tempdir().unwrap();
    let home = t.path().canonicalize().unwrap();
    std::fs::create_dir_all(home.join(".ssh")).unwrap();
    std::fs::write(home.join(".ssh/id_x"), b"k").unwrap();
    std::fs::write(home.join("doc.txt"), b"d").unwrap();
    let deny = deny_for(&home);
    assert_eq!(
        validate(&deny, &[home.join("doc.txt")]).unwrap(),
        vec![home.join("doc.txt").to_str().unwrap().to_owned()]
    );
    assert_eq!(validate(&deny, &[home.join(".ssh/id_x")]), Err(PROTECTED));
    assert_eq!(
        validate(&deny, &[home.join("doc.txt"), home.join(".ssh")]),
        Err(PROTECTED)
    );
    assert_eq!(validate(&deny, &[home.join("nope")]), Err(MISSING));
    assert_eq!(
        validate(&deny, &[PathBuf::from("/etc/hostname")]),
        Err(PROTECTED)
    );
    assert_eq!(validate(&deny, &[]), Err(NOTHING_STAGED));
}

#[test]
fn symlink_into_a_protected_folder_is_refused() {
    let t = tempfile::tempdir().unwrap();
    let home = t.path().canonicalize().unwrap();
    std::fs::create_dir_all(home.join(".ssh")).unwrap();
    std::fs::write(home.join(".ssh/id_x"), b"k").unwrap();
    std::os::unix::fs::symlink(home.join(".ssh/id_x"), home.join("innocent.txt")).unwrap();
    assert_eq!(
        validate(&deny_for(&home), &[home.join("innocent.txt")]),
        Err(PROTECTED)
    );
}

#[test]
fn drop_urls_accept_only_local_file_urls() {
    assert_eq!(
        urls_to_paths("file:///home/a/b%20c.txt\nfile://localhost/x").unwrap(),
        vec![PathBuf::from("/home/a/b c.txt"), PathBuf::from("/x")]
    );
    assert_eq!(urls_to_paths("https://example.test/a"), Err(NOT_LOCAL));
    assert_eq!(urls_to_paths("file://host/x"), Err(NOT_LOCAL));
    // One foreign URL refuses the whole drop.
    assert_eq!(urls_to_paths("file:///a\nsmb://h/b"), Err(NOT_LOCAL));
    assert_eq!(
        urls_to_paths("file:///a/../b\r\n"),
        Ok(vec![PathBuf::from("/a/../b")])
    );
    assert_eq!(urls_to_paths(""), Err(NOT_LOCAL));
    assert_eq!(urls_to_paths("file:///a%00b"), Err(NOT_LOCAL));
}

#[test]
fn refusal_codes_map_to_their_sentences() {
    assert!(refusal_text(PROTECTED).contains("protected"));
    assert_eq!(refusal_text(MISSING), "A file no longer exists.");
    assert_eq!(
        refusal_text(NOT_LOCAL),
        "Only files on this computer can be sent."
    );
    assert_eq!(refusal_text(NOTHING_STAGED), "There is nothing to send.");
    assert_eq!(refusal_text("unknown-code"), "The files could not be sent.");
}

#[test]
fn relative_paths_are_refused_as_missing() {
    let t = tempfile::tempdir().unwrap();
    let home = t.path().canonicalize().unwrap();
    std::fs::write(home.join("doc.txt"), b"d").unwrap();
    assert_eq!(
        validate(&deny_for(&home), &[PathBuf::from("doc.txt")]),
        Err(MISSING)
    );
}

#[test]
fn sending_a_folder_that_contains_a_protected_root_is_refused() {
    // A drop of the whole home folder would expand into ~/.ssh: refused as a whole.
    let t = tempfile::tempdir().unwrap();
    let home = t.path().canonicalize().unwrap();
    std::fs::create_dir_all(home.join(".ssh")).unwrap();
    std::fs::write(home.join(".ssh/id_x"), b"k").unwrap();
    assert_eq!(
        validate(&deny_for(&home), std::slice::from_ref(&home)),
        Err(PROTECTED)
    );
}

#[test]
fn more_than_the_item_cap_is_refused_and_the_cap_itself_is_allowed() {
    let t = tempfile::tempdir().unwrap();
    let home = t.path().canonicalize().unwrap();
    let f = home.join("doc.txt");
    std::fs::write(&f, b"d").unwrap();
    let deny = deny_for(&home);
    let over = vec![f.clone(); MAX_PATHS + 1];
    assert_eq!(validate(&deny, &over), Err(TOO_MANY));
    assert_eq!(
        validate(&deny, &over[..MAX_PATHS]).map(|p| p.len()),
        Ok(MAX_PATHS)
    );
}

#[test]
fn a_name_that_is_not_utf8_is_refused_not_altered() {
    use std::os::unix::ffi::OsStringExt;
    let t = tempfile::tempdir().unwrap();
    let home = t.path().canonicalize().unwrap();
    let odd = home.join(std::ffi::OsString::from_vec(b"bad-\xff.txt".to_vec()));
    std::fs::write(&odd, b"d").unwrap();
    // Its lossy twin does not exist: with a lossy conversion core would see another file.
    assert_eq!(validate(&deny_for(&home), &[odd]), Err(UNSUPPORTED));
}

#[test]
fn encoded_drop_urls_keep_spaces_and_special_characters_in_names() {
    // "report " (trailing space), "a#b", "100%": all must survive one decode and no trim.
    let got = urls_to_paths("file:///h/report%20\nfile:///h/a%23b\nfile:///h/100%25").unwrap();
    assert_eq!(
        got,
        vec![
            PathBuf::from("/h/report "),
            PathBuf::from("/h/a#b"),
            PathBuf::from("/h/100%")
        ]
    );
    // A raw space is not part of a valid URL: it is line whitespace, like in any uri-list.
    assert_eq!(
        urls_to_paths("file:///h/x "),
        Ok(vec![PathBuf::from("/h/x")])
    );
    assert_eq!(urls_to_paths("file:///h/%zz"), Err(NOT_LOCAL));
    assert_eq!(urls_to_paths("file:///h/%+1"), Err(NOT_LOCAL));
}

#[test]
fn summary_names_count_and_size_and_marks_folders() {
    let meta = |p: &Path| (p != Path::new("/h/dir")).then_some(1536);
    let one = vec!["/h/a.pdf".to_string()];
    assert_eq!(summarize(&one, meta), "1 item: a.pdf (1.5 KiB)");
    let many: Vec<String> = ["a", "b", "c", "d", "e"]
        .map(|n| format!("/h/{n}"))
        .to_vec();
    assert_eq!(
        summarize(&many, meta),
        "5 items: a, b, c and 2 more (7.5 KiB)"
    );
    let mixed = vec!["/h/dir".to_string(), "/h/f".to_string()];
    assert_eq!(
        summarize(&mixed, meta),
        "2 items: dir, f (1.5 KiB, plus 1 folder)"
    );
    assert_eq!(
        summarize(&["/h/dir".to_string()], meta),
        "1 item: dir (folders)"
    );
}

#[test]
fn send_event_text_covers_the_final_states_only() {
    assert_eq!(send_event_text(TransferState::Done), Some("Files sent."));
    assert_eq!(
        send_event_text(TransferState::Failed),
        Some("Sending failed.")
    );
    assert_eq!(
        send_event_text(TransferState::Cancelled),
        Some("Sending cancelled.")
    );
    assert_eq!(send_event_text(TransferState::Progress), None);
}

#[test]
fn every_refusal_code_has_its_own_sentence() {
    for code in [
        PROTECTED,
        MISSING,
        UNSUPPORTED,
        NOT_LOCAL,
        NOTHING_STAGED,
        TOO_MANY,
        STALE,
        SIGNED_OUT,
    ] {
        assert_ne!(refusal_text(code), refusal_text("other"), "{code}");
    }
}

#[test]
fn a_verdict_older_than_the_latest_request_changes_nothing() {
    let t = tempfile::tempdir().unwrap();
    let home = t.path().canonicalize().unwrap();
    std::fs::write(home.join("a.txt"), b"a").unwrap();
    std::fs::write(home.join("b.txt"), b"b").unwrap();
    let c = core();
    let deny = deny_for(&home);
    let slow = lock(&c.pending_send).begin_request();
    let newer = lock(&c.pending_send).begin_request();
    // The newer one finishes first and is shown.
    assert_eq!(stage_with(&c, &deny, newer, &[home.join("b.txt")]), Ok(1));
    let shown = lock(&c.pending_send).clone();
    // The slow earlier check completes later: it must neither replace nor clear the set,
    // whether it succeeds or is refused.
    assert_eq!(
        stage_with(&c, &deny, slow, &[home.join("a.txt")]),
        Err(SUPERSEDED)
    );
    assert_eq!(
        stage_with(&c, &deny, slow, &[home.join("missing")]),
        Err(SUPERSEDED)
    );
    assert_eq!(*lock(&c.pending_send), shown);
    assert!(shown.paths[0].ends_with("b.txt"));
}

#[test]
fn a_refused_latest_request_empties_the_set_and_sign_out_voids_running_checks() {
    let t = tempfile::tempdir().unwrap();
    let home = t.path().canonicalize().unwrap();
    std::fs::write(home.join("a.txt"), b"a").unwrap();
    let c = core();
    let deny = deny_for(&home);
    let r1 = lock(&c.pending_send).begin_request();
    assert_eq!(stage_with(&c, &deny, r1, &[home.join("a.txt")]), Ok(1));
    let r2 = lock(&c.pending_send).begin_request();
    assert_eq!(
        stage_with(&c, &deny, r2, &[home.join("nope")]),
        Err(MISSING)
    );
    assert!(lock(&c.pending_send).is_empty());
    let r3 = lock(&c.pending_send).begin_request();
    super::reset_for_sign_out(&c);
    assert_eq!(
        stage_with(&c, &deny, r3, &[home.join("a.txt")]),
        Err(SUPERSEDED)
    );
    assert!(lock(&c.pending_send).is_empty());
}

#[test]
fn too_many_checks_at_once_are_refused_busy() {
    let c = core();
    c.stage_inflight.store(MAX_INFLIGHT, Ordering::SeqCst);
    assert_eq!(
        stage_and_show(&c, vec![PathBuf::from("/x")], None, None),
        Err(BUSY)
    );
    assert_eq!(c.stage_inflight.load(Ordering::SeqCst), MAX_INFLIGHT);
}

#[test]
fn a_closed_gate_never_runs_the_check_and_frees_its_slot() {
    let c = core();
    c.gate.close();
    // The work is dropped unrun: the waiter sees a disconnect (`shutting-down`).
    let (tx, rx) = std::sync::mpsc::channel();
    assert_eq!(
        stage_and_show(&c, vec![PathBuf::from("/x")], None, Some(tx)),
        Ok(())
    );
    assert!(rx.recv().is_err());
    assert_eq!(c.stage_inflight.load(Ordering::SeqCst), 0);
    assert_eq!(
        stage_bounded(&c, vec![PathBuf::from("/x")], None),
        Err(SHUTTING_DOWN)
    );
}

#[test]
fn cold_start_waits_or_refuses_with_the_sentence_of_the_real_state() {
    assert_eq!(cold_start(EngineState::Ready, false), ColdStart::Stage);
    assert_eq!(cold_start(EngineState::Starting, false), ColdStart::Wait);
    assert_eq!(
        cold_start(EngineState::WaitingForKeyring, false),
        ColdStart::Wait
    );
    assert_eq!(cold_start(EngineState::Offline, false), ColdStart::Wait);
    assert_eq!(
        cold_start(EngineState::SignedOut, false),
        ColdStart::Refuse(SIGNED_OUT)
    );
    assert_eq!(
        cold_start(EngineState::AwaitingCode, false),
        ColdStart::Refuse(SIGNED_OUT)
    );
    assert_eq!(
        cold_start(EngineState::NeedsPairing, false),
        ColdStart::Refuse(NEEDS_PAIRING)
    );
    assert_eq!(
        cold_start(EngineState::Error, false),
        ColdStart::Refuse(NOT_READY)
    );
    assert_eq!(
        cold_start(EngineState::WaitingForKeyring, true),
        ColdStart::Refuse(KEYRING_LOCKED)
    );
    assert_eq!(
        cold_start(EngineState::Offline, true),
        ColdStart::Refuse(OFFLINE)
    );
    assert_eq!(
        cold_start(EngineState::Starting, true),
        ColdStart::Refuse(NOT_READY)
    );
    // Each refusal has its own sentence, and none says "sign in" except the signed-out one.
    for code in [
        NEEDS_PAIRING,
        KEYRING_LOCKED,
        OFFLINE,
        NOT_READY,
        SHUTTING_DOWN,
        BUSY,
    ] {
        assert_ne!(refusal_text(code), refusal_text("x"), "{code}");
        assert!(!refusal_text(code).starts_with("Sign in"), "{code}");
    }
    assert!(refusal_text(KEYRING_LOCKED).contains("keyring"));
}
