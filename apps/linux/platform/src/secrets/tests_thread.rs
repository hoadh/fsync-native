use super::super::tests_open_store::FakeBus;
use super::super::{SecretStore, SessionTokens, file_fallback};
use super::*;

fn sample() -> Secrets {
    Secrets {
        device_secret: [7; 32],
        history_key: [8; 32],
        session: Some(SessionTokens {
            access: "a".into(),
            refresh: "r".into(),
            account_id: "i".into(),
        }),
        email: None,
        pending_revoke: Some("old".into()),
    }
}

#[test]
fn file_roundtrip_through_thread_with_0600() {
    use std::os::unix::fs::MetadataExt;
    let d = tempfile::tempdir().unwrap();
    let h =
        SecretsHandle::spawn(d.path().into(), Arc::new(FakeBus::new(false, false)), false).unwrap();
    let st = h.ensure_open().unwrap();
    assert!(st.file_fallback_warning && !st.waiting_for_keyring);
    assert!(h.load().unwrap().is_none());
    h.save(sample(), Duration::from_secs(3)).unwrap();
    let mode = std::fs::metadata(d.path().join(file_fallback::FILE_NAME))
        .unwrap()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    let got = h.load().unwrap().unwrap();
    assert_eq!(got.device_secret, [7; 32]);
    assert_eq!(got.pending_revoke.as_deref(), Some("old"));
    h.clear().unwrap();
    assert!(h.load().unwrap().is_none());
}

#[test]
fn waiting_state_returns_error_and_creates_nothing_then_recovers() {
    let d = tempfile::tempdir().unwrap();
    super::super::backend_marker::write_marker(d.path(), Backend::SecretService).unwrap();
    let h =
        SecretsHandle::spawn(d.path().into(), Arc::new(FakeBus::new(false, false)), false).unwrap();
    assert!(h.ensure_open().unwrap().waiting_for_keyring);
    assert!(matches!(h.load(), Err(SecretsError::WaitingForKeyring)));
    assert!(matches!(
        h.save(sample(), Duration::from_secs(3)),
        Err(SecretsError::WaitingForKeyring)
    ));
    assert!(!d.path().join(file_fallback::FILE_NAME).exists());
    // Keyring comes back (new handle with a usable bus; marker unchanged).
    let h2 =
        SecretsHandle::spawn(d.path().into(), Arc::new(FakeBus::new(true, true)), false).unwrap();
    assert!(!h2.ensure_open().unwrap().waiting_for_keyring);
}

#[test]
fn file_store_refuses_wide_mode_and_foreign_symlink() {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    let mut store = file_fallback::FileStore::new(d.path());
    store.save(&sample()).unwrap();
    let p = d.path().join(file_fallback::FILE_NAME);
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(store.load(), Err(SecretsError::Insecure(_))));
    std::fs::remove_file(&p).unwrap();
    std::os::unix::fs::symlink("/etc/hostname", &p).unwrap();
    assert!(store.load().is_err());
}

#[test]
fn applied_save_seq_advances_even_when_the_caller_gave_up_and_observer_fires() {
    use std::sync::atomic::{AtomicU32, Ordering as O};
    let d = tempfile::tempdir().unwrap();
    let h =
        SecretsHandle::spawn(d.path().into(), Arc::new(FakeBus::new(false, false)), false).unwrap();
    let seen = Arc::new(AtomicU32::new(0));
    let s2 = Arc::clone(&seen);
    h.set_save_observer(Arc::new(move || {
        s2.fetch_add(1, O::SeqCst);
    }));
    assert_eq!(h.applied_save_seq(), 0);
    let (seq, reply) = h.save_queued(sample());
    // The caller gives up at once (a timeout in real life); the save still completes.
    let _ = reply.unwrap().wait(Duration::ZERO);
    let start = std::time::Instant::now();
    while h.applied_save_seq() < seq && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(h.applied_save_seq(), seq);
    while seen.load(O::SeqCst) == 0 && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(seen.load(O::SeqCst) >= 1);
    let (seq2, r2) = h.save_queued(sample());
    r2.unwrap().wait(Duration::from_secs(3)).unwrap();
    assert!(seq2 > seq && h.applied_save_seq() == seq2);
}
