//! `Inner::stop` with a leaked engine handle: the lock is held for the bounded time, then the
//! engine is closed and `engine.lock` is released.

use super::*;
use fsync_proto::Platform;
use std::{sync::Arc, time::Duration};
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

fn service(dir: &std::path::Path) -> FsyncService {
    let cfg = ServiceConfig {
        data_dir: dir.to_owned(),
        api_base: "http://127.0.0.1:9".into(),
        relay_urls: vec![],
        device_name: "leak-test".into(),
        platform: Platform::Mac,
        device_secret: Zeroizing::new([3; 32]),
        history_key: Zeroizing::new([7; 32]),
        force_relay: false,
        allow_insecure_http: true,
        mdns: false,
    };
    FsyncService::new(cfg, Arc::new(Nop)).unwrap()
}

fn lock_is_free(dir: &std::path::Path) -> bool {
    let f = std::fs::File::create(dir.join("engine.lock")).unwrap();
    f.try_lock().is_ok()
}

fn wait_until_lock_free(dir: &std::path::Path) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !lock_is_free(dir) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    lock_is_free(dir)
}

/// The hold is far longer than the test, so "still held, not closed" is observed
/// deterministically; releasing the handle then lets the holder shut down cleanly.
#[test]
fn lock_is_held_while_a_handle_is_leaked_and_freed_when_it_drops() {
    let dir = tempfile::tempdir().unwrap();
    let svc = service(dir.path());
    svc.start().unwrap();
    let leaked = svc.inner.engine().unwrap();

    svc.run(async {
        svc.inner
            .stop_with(Duration::from_millis(50), Duration::from_secs(300))
            .await;
        Ok(())
    })
    .unwrap();
    assert!(!lock_is_free(dir.path()), "held while the handle is leaked");
    assert!(!leaked.node().is_closed());

    drop(leaked);
    assert!(
        wait_until_lock_free(dir.path()),
        "freed after the last drop"
    );
    svc.start().expect("restart works");
    svc.stop().unwrap();
}

/// After the (short) hold the leaked engine is closed before the lock is released, so once
/// the lock is free the node is already closed; no timing assumption beyond a generous wait.
#[test]
fn leaked_engine_is_closed_before_the_lock_is_released_on_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let svc = service(dir.path());
    svc.start().unwrap();
    let leaked = svc.inner.engine().unwrap();

    svc.run(async {
        svc.inner
            .stop_with(Duration::from_millis(50), Duration::from_millis(300))
            .await;
        Ok(())
    })
    .unwrap();
    assert!(
        wait_until_lock_free(dir.path()),
        "lock released after the hold"
    );
    assert!(
        leaked.node().is_closed(),
        "closed before the lock was released"
    );

    // Dropping the last handle after close must be harmless, and a restart works.
    drop(leaked);
    svc.start().unwrap();
    svc.stop().unwrap();
}
