//! One engine per data directory: a second `start` on the same dir is refused until the first
//! one stops or is dropped.

use fsync_core::service::*;
use fsync_proto::Platform;
use std::sync::Arc;
use zeroize::Zeroizing;

struct Nop;
impl ServiceDelegate for Nop {
    fn on_remote_clip(&self, _: ClipItem) {}
    fn on_device_status(&self, _: Device) {}
    fn on_transfer(&self, _: TransferEvent) {}
    fn on_incoming_files(&self, _: ClipItem) {}
    fn on_pairing_event(&self, _: PairingEvent) {}
    fn on_account_event(&self, _: AccountEvent) {}
    fn on_session_changed(&self, _: SessionTokens) {}
    fn on_error(&self, _: FsyncError) {}
}

fn service(dir: &std::path::Path, seed: u8) -> FsyncService {
    let cfg = ServiceConfig {
        data_dir: dir.to_owned(),
        api_base: "http://127.0.0.1:9".into(),
        relay_urls: vec![],
        device_name: "lock-test".into(),
        platform: Platform::Mac,
        device_secret: Zeroizing::new([seed; 32]),
        history_key: Zeroizing::new([7; 32]),
        force_relay: false,
        allow_insecure_http: true,
        mdns: false,
    };
    FsyncService::new(cfg, Arc::new(Nop)).unwrap()
}

#[test]
fn second_engine_on_the_same_data_dir_is_refused_until_the_first_stops() {
    let dir = tempfile::tempdir().unwrap();
    let a = service(dir.path(), 1);
    a.start().unwrap();
    a.start()
        .expect("start is idempotent on the owning service");

    let b = service(dir.path(), 1);
    let err = b.start().unwrap_err();
    assert!(
        matches!(&err, FsyncError::Invalid(m) if m.contains("already running")),
        "{err:?}"
    );

    a.stop().unwrap();
    b.start().expect("free after the first engine stopped");
    assert!(matches!(a.start(), Err(FsyncError::Invalid(_))));
    b.stop().unwrap();
    a.start().expect("start works again after stop");
}

#[test]
fn the_lock_is_released_when_the_service_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let a = service(dir.path(), 2);
    a.start().unwrap();
    drop(a);
    let b = service(dir.path(), 2);
    b.start().expect("dropped service freed the dir");
    b.stop().unwrap();
}

#[test]
fn a_different_data_dir_is_independent() {
    let (d1, d2) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (a, b) = (service(d1.path(), 3), service(d2.path(), 4));
    a.start().unwrap();
    b.start().unwrap();
    a.stop().unwrap();
    b.stop().unwrap();
}

#[test]
fn concurrent_starts_on_one_service_all_succeed() {
    let dir = tempfile::tempdir().unwrap();
    let a = Arc::new(service(dir.path(), 5));
    let hs: Vec<_> = (0..6)
        .map(|_| {
            let a = a.clone();
            std::thread::spawn(move || a.start())
        })
        .collect();
    for h in hs {
        h.join().unwrap().expect("no spurious Invalid");
    }
    a.stop().unwrap();
}
