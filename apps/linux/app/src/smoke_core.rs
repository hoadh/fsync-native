//! `FSYNC_SMOKE_CORE=1 --smoke`: proves fsync-core links and opens its encrypted history
//! (vendored SQLCipher/OpenSSL) on this system. Throwaway keys in a temp dir, no network.

use fsync_core::service::{
    AccountEvent, ClipItem, Device, FsyncError, FsyncService, PairingEvent, ServiceConfig,
    ServiceDelegate, SessionTokens, TransferEvent,
};
use std::sync::Arc;
use zeroize::Zeroizing;

struct NoopDelegate;

impl ServiceDelegate for NoopDelegate {
    fn on_remote_clip(&self, _: ClipItem) {}
    fn on_device_status(&self, _: Device) {}
    fn on_transfer(&self, _: TransferEvent) {}
    fn on_incoming_files(&self, _: ClipItem) {}
    fn on_pairing_event(&self, _: PairingEvent) {}
    fn on_account_event(&self, _: AccountEvent) {}
    fn on_session_changed(&self, _: SessionTokens) {}
    fn on_error(&self, _: FsyncError) {}
}

pub fn open_and_close() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
    let cfg = ServiceConfig {
        data_dir: dir.path().join("data"),
        api_base: "http://127.0.0.1:9".into(),
        relay_urls: Vec::new(),
        device_name: "smoke".into(),
        platform: fsync_proto::Platform::Cli,
        device_secret: Zeroizing::new([7u8; 32]),
        history_key: Zeroizing::new([9u8; 32]),
        force_relay: false,
        allow_insecure_http: true,
        mdns: false,
    };
    let svc = FsyncService::new(cfg, Arc::new(NoopDelegate)).map_err(|e| e.to_string())?;
    drop(svc);
    Ok(())
}
