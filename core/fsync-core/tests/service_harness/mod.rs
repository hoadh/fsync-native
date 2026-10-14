//! Shared helpers for the service-level trust-reset tests (real backend, real engines).
#![allow(dead_code)]

use crate::backend_harness::Backend;
use fsync_core::service::*;
use fsync_proto::Platform;
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub const EMAIL: &str = "a@example.com";

#[derive(Default)]
pub struct Rec {
    pub accounts: Mutex<Vec<AccountEvent>>,
    pub pairing: Mutex<Vec<PairingEvent>>,
}

impl Rec {
    pub fn resets(&self) -> Vec<(String, u32)> {
        let a = self.accounts.lock().unwrap();
        a.iter()
            .filter_map(|e| match e {
                AccountEvent::TrustReset {
                    reset_id, devices, ..
                } => Some((reset_id.clone(), *devices)),
                _ => None,
            })
            .collect()
    }
    /// Names offered by each `TrustReset` event, in order.
    pub fn reset_names(&self) -> Vec<Vec<String>> {
        let a = self.accounts.lock().unwrap();
        a.iter()
            .filter_map(|e| match e {
                AccountEvent::TrustReset { device_names, .. } => Some(device_names.clone()),
                _ => None,
            })
            .collect()
    }
    pub fn count(&self, f: impl Fn(&AccountEvent) -> bool) -> usize {
        self.accounts
            .lock()
            .unwrap()
            .iter()
            .filter(|e| f(e))
            .count()
    }
    pub fn confirm_request(&self) -> Option<PairingPeer> {
        self.pairing.lock().unwrap().iter().find_map(|e| match e {
            PairingEvent::ConfirmRequest(p) => Some(p.clone()),
            _ => None,
        })
    }
    pub fn completed(&self) -> bool {
        let p = self.pairing.lock().unwrap();
        p.iter()
            .any(|e| matches!(e, PairingEvent::Completed { .. }))
    }
}

impl ServiceDelegate for Rec {
    fn on_remote_clip(&self, _: ClipItem) {}
    fn on_device_status(&self, _: Device) {}
    fn on_transfer(&self, _: TransferEvent) {}
    fn on_incoming_files(&self, _: ClipItem) {}
    fn on_pairing_event(&self, e: PairingEvent) {
        self.pairing.lock().unwrap().push(e);
    }
    fn on_account_event(&self, e: AccountEvent) {
        self.accounts.lock().unwrap().push(e);
    }
    fn on_session_changed(&self, _: SessionTokens) {}
    fn on_error(&self, _: FsyncError) {}
}

pub struct Dev {
    pub svc: FsyncService,
    pub rec: Arc<Rec>,
    _dir: tempfile::TempDir,
}

/// Pairing finds the peer through mDNS on this host, so devices that pair need it.
pub fn dev(be: &Backend, seed: u8, name: &str) -> Dev {
    dev_with(be, seed, name, true)
}

pub fn dev_with(be: &Backend, seed: u8, name: &str, mdns: bool) -> Dev {
    let dir = tempfile::tempdir().unwrap();
    let rec = Arc::new(Rec::default());
    let cfg = ServiceConfig {
        data_dir: Path::new(dir.path()).to_owned(),
        api_base: be.base.clone(),
        relay_url: None,
        device_name: name.into(),
        platform: Platform::Mac,
        device_secret: Zeroizing::new([seed; 32]),
        history_key: Zeroizing::new([seed; 32]),
        force_relay: false,
        allow_insecure_http: true,
        mdns,
    };
    let svc = FsyncService::new(cfg, rec.clone()).unwrap();
    Dev {
        svc,
        rec,
        _dir: dir,
    }
}

impl Dev {
    /// Fresh OTP login (the backend then sees a recent `auth_time`).
    pub fn login(&self, be: &Backend) -> SessionTokens {
        self.svc.login_start(EMAIL).unwrap();
        self.svc.login_verify(EMAIL, &be.last_code(EMAIL)).unwrap()
    }
}

pub fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(20);
    while !f() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

static LAN: Mutex<()> = Mutex::new(());

/// Devices with mDNS find each other on the local network, so tests that start such devices
/// would answer each other's pairing codes: hold this for the whole test (one at a time per
/// test binary).
///
/// The lock is a static of this test binary only: separate binaries run as separate processes
/// (and runners like nextest run each test in its own process), so on one LAN they can still
/// collide. Run these tests with `cargo test`, or one at a time.
pub fn lan_serial() -> std::sync::MutexGuard<'static, ()> {
    LAN.lock().unwrap_or_else(|p| p.into_inner())
}

/// `newcomer` offers a code, `existing` joins it; both confirm the words.
pub fn pair(existing: &Dev, newcomer: &Dev) {
    newcomer.svc.start().unwrap();
    existing.svc.start().unwrap();
    let code = newcomer
        .svc
        .pairing_offer(PairingMode::Code)
        .unwrap()
        .code
        .unwrap();
    let theirs = existing.svc.pairing_join(&code).unwrap();
    wait_for("confirm request", || {
        newcomer.rec.confirm_request().is_some()
    });
    let mine = newcomer.rec.confirm_request().unwrap();
    assert_eq!(mine.sas_words, theirs.sas_words);
    existing
        .svc
        .pairing_confirm(theirs.request_id, true)
        .unwrap();
    newcomer.svc.pairing_confirm(mine.request_id, true).unwrap();
    wait_for("pairing completed", || {
        newcomer.rec.completed() && existing.rec.completed()
    });
}

pub fn start_account(be: &Backend, seed: u8, name: &str) -> Dev {
    start_account_with(be, seed, name, true)
}

pub fn start_account_with(be: &Backend, seed: u8, name: &str, mdns: bool) -> Dev {
    let d = dev_with(be, seed, name, mdns);
    d.login(be);
    assert_eq!(d.svc.ensure_account().unwrap(), AccountState::Created);
    d.svc.start().unwrap();
    d
}
