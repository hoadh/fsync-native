//! `FsyncService` against the real backend: sign-in, genesis, devices, history, settings,
//! callbacks and key-loss handling.

mod backend_harness;
mod common;
use backend_harness::Backend;
use fsync_core::service::*;
use fsync_proto::Platform;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

#[derive(Default)]
struct Rec {
    sessions: Mutex<Vec<SessionTokens>>,
    accounts: Mutex<Vec<AccountEvent>>,
    thread_names: Mutex<Vec<String>>,
}

impl ServiceDelegate for Rec {
    fn on_remote_clip(&self, _: ClipItem) {}
    fn on_device_status(&self, _: Device) {}
    fn on_transfer(&self, _: TransferEvent) {}
    fn on_incoming_files(&self, _: ClipItem) {}
    fn on_pairing_event(&self, _: PairingEvent) {}
    fn on_account_event(&self, e: AccountEvent) {
        self.thread_names
            .lock()
            .unwrap()
            .push(std::thread::current().name().unwrap_or("").into());
        self.accounts.lock().unwrap().push(e);
    }
    fn on_session_changed(&self, t: SessionTokens) {
        self.sessions.lock().unwrap().push(t);
    }
    fn on_error(&self, _: FsyncError) {}
}

fn cfg(dir: &std::path::Path, api: &str, seed: u8, hist: u8) -> ServiceConfig {
    ServiceConfig {
        data_dir: dir.to_owned(),
        api_base: api.to_owned(),
        relay_urls: vec![],
        device_name: "test-mac".into(),
        platform: Platform::Mac,
        device_secret: Zeroizing::new([seed; 32]),
        history_key: Zeroizing::new([hist; 32]),
        force_relay: false,
        allow_insecure_http: true,
        mdns: false,
    }
}

fn wait_for(mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !f() {
        assert!(Instant::now() < end, "timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}

// The service owns its runtime and blocks, so the test itself must not run inside one.
#[test]
fn first_device_flow_history_settings_and_callbacks() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let be = rt.block_on(Backend::start());
    let dir = tempfile::tempdir().unwrap();
    let rec = Arc::new(Rec::default());
    let svc = FsyncService::new(cfg(dir.path(), &be.base, 7, 9), rec.clone()).unwrap();

    // Not signed in: nothing to create.
    assert_eq!(svc.ensure_account().unwrap_err(), FsyncError::Auth);
    assert!(svc.devices().is_empty());

    svc.login_start("a@example.com").unwrap();
    assert!(svc.login_verify("a@example.com", "000000").is_err());
    let code = be.last_code("a@example.com");
    let tokens = svc.login_verify("a@example.com", &code).unwrap();
    assert!(!tokens.access.is_empty() && !tokens.account_id.is_empty());
    wait_for(|| !rec.sessions.lock().unwrap().is_empty());

    assert_eq!(svc.ensure_account().unwrap(), AccountState::Created);
    let devs = svc.devices();
    assert_eq!(devs.len(), 1);
    assert!(devs[0].is_self && devs[0].name == "test-mac");
    assert_eq!(devs[0].node_id, svc.node_id());

    svc.start().unwrap();
    let published = svc
        .publish_clip(LocalClip {
            reps: vec![ClipRep {
                mime: "text/plain".into(),
                data: b"hello".to_vec(),
            }],
            files: vec![],
            preview: None,
            sensitive: false,
        })
        .unwrap();
    assert!(!published.echo && published.delivered.is_empty());
    let page = svc.history(None, None, 10).unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(page.items[0].outgoing && page.items[0].preview.as_deref() == Some("hello"));
    assert_eq!(
        svc.history(Some("hell".into()), None, 10)
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(svc.history(None, Some("garbage".into()), 10).is_err());

    // The size cap applies to the running engine at once.
    let mut s = svc.settings().unwrap();
    s.max_clip_bytes = 4;
    svc.set_settings(s).unwrap();
    let big = LocalClip {
        reps: vec![ClipRep {
            mime: "text/plain".into(),
            data: b"too big".to_vec(),
        }],
        files: vec![],
        preview: None,
        sensitive: false,
    };
    assert_eq!(svc.publish_clip(big).unwrap_err(), FsyncError::TooLarge);

    // Cannot remove oneself; unknown / malformed ids are rejected.
    assert!(svc.remove_device(&svc.node_id()).is_err());
    assert!(svc.remove_device("zz").is_err());
    assert!(svc.pairing_confirm(1, true).is_err());

    // One offer window at a time; the second offer reports it, cancel frees it.
    svc.pairing_offer(PairingMode::Qr).unwrap();
    assert!(svc.pairing_offer(PairingMode::Qr).is_err());
    svc.pairing_cancel().unwrap();
    svc.pairing_offer(PairingMode::Qr).unwrap();
    svc.pairing_cancel().unwrap();

    // Sign in again from a stored session on a fresh service: same account, Ready.
    svc.stop().unwrap();
    drop(svc);
    let rec2 = Arc::new(Rec::default());
    let svc2 = FsyncService::new(cfg(dir.path(), &be.base, 7, 9), rec2).unwrap();
    svc2.set_session(tokens).unwrap();
    assert_eq!(svc2.ensure_account().unwrap(), AccountState::Ready);
    svc2.logout().unwrap();
    assert!(svc2.devices().is_empty());
}

#[test]
fn a_different_history_key_is_refused_and_a_damaged_file_is_reset_on_the_callback_thread() {
    let dir = tempfile::tempdir().unwrap();
    let open = |key: u8, rec: Arc<Rec>| {
        FsyncService::new(cfg(dir.path(), "http://127.0.0.1:9", 7, key), rec)
    };
    drop(open(9, Arc::new(Rec::default())).unwrap());
    // Wrong key: a distinct error, nothing moved aside, the right key still works.
    assert_eq!(
        open(10, Arc::new(Rec::default())).err(),
        Some(FsyncError::HistoryKeyMismatch)
    );
    drop(open(9, Arc::new(Rec::default())).unwrap());

    // The explicit abandon path: reset, then the other key opens a fresh history.
    let other = tempfile::tempdir().unwrap();
    drop(
        FsyncService::new(
            cfg(other.path(), "http://127.0.0.1:9", 7, 9),
            Arc::new(Rec::default()),
        )
        .unwrap(),
    );
    let c10 = || cfg(other.path(), "http://127.0.0.1:9", 7, 10);
    assert_eq!(
        FsyncService::new(c10(), Arc::new(Rec::default())).err(),
        Some(FsyncError::HistoryKeyMismatch)
    );
    FsyncService::reset_history(other.path()).unwrap();
    FsyncService::new(c10(), Arc::new(Rec::default())).expect("opens after reset");

    // A file that is damaged under its own key is moved aside and reported.
    std::fs::write(dir.path().join("history.db"), vec![0x5au8; 20_000]).unwrap();
    let _ = std::fs::remove_file(dir.path().join("history.db-wal"));
    let rec = Arc::new(Rec::default());
    let _svc = open(9, rec.clone()).unwrap();
    wait_for(|| !rec.accounts.lock().unwrap().is_empty());
    assert!(matches!(
        rec.accounts.lock().unwrap()[0],
        AccountEvent::HistoryReset { .. }
    ));
    assert_eq!(rec.thread_names.lock().unwrap()[0], "fsync-callbacks");
}

#[test]
fn bad_key_lengths_and_urls_are_rejected() {
    assert!(key32(&[0; 31], "history key").is_err());
    assert!(key32(&[0; 32], "history key").is_ok());
    let dir = tempfile::tempdir().unwrap();
    let mut c = cfg(dir.path(), "http://example.com", 1, 1);
    c.allow_insecure_http = false;
    assert!(FsyncService::new(c, Arc::new(Rec::default())).is_err());
}

/// Blocking calls from async code are an error, and dropping inside a runtime must not panic.
#[test]
fn service_is_safe_inside_a_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let svc = FsyncService::new(
            cfg(dir.path(), "http://127.0.0.1:9", 7, 9),
            Arc::new(Rec::default()),
        )
        .unwrap();
        assert!(matches!(svc.start(), Err(FsyncError::Internal(_))));
        drop(svc);
    });
}

#[test]
fn pin_and_delete_of_an_unknown_item_are_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let svc = FsyncService::new(
        cfg(dir.path(), "http://127.0.0.1:9", 7, 9),
        Arc::new(Rec::default()),
    )
    .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    assert_eq!(svc.pin(&id, true), Err(FsyncError::NotFound));
    assert_eq!(svc.delete(&id), Err(FsyncError::NotFound));
}
