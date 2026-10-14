use crate::{
    convert::{FfiResult, map, unit},
    delegate::{Bridge, EngineDelegate},
    error::FsyncError,
    types_account::*,
    types_clip::*,
    types_device::*,
    types_history::*,
};
use fsync_core::service::{self as svc, FsyncService, ServiceConfig, key32};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, uniffi::Record)]
pub struct EngineConfig {
    /// Holds `history.db` and `blobs/`.
    pub data_dir: String,
    pub api_base: String,
    /// Our relay (none: LAN only).
    pub relay_url: Option<String>,
    pub device_name: String,
    pub platform: Platform,
    /// 32 bytes, from Keychain / DPAPI.
    pub device_secret: Vec<u8>,
    /// 32 bytes, from Keychain / DPAPI.
    pub history_key: Vec<u8>,
    /// Test flag: relay only, no direct paths.
    pub force_relay: bool,
    /// Dev backends on plain http.
    pub allow_insecure_http: bool,
}

#[derive(uniffi::Object)]
pub struct FsyncEngine {
    svc: FsyncService,
}

#[uniffi::export]
impl FsyncEngine {
    /// Opens history and blob store; the network stays off until `start`.
    #[uniffi::constructor]
    pub fn new(cfg: EngineConfig, delegate: Arc<dyn EngineDelegate>) -> FfiResult<Arc<Self>> {
        let sc = ServiceConfig {
            data_dir: PathBuf::from(cfg.data_dir),
            api_base: cfg.api_base,
            relay_url: cfg.relay_url,
            device_name: cfg.device_name,
            platform: cfg.platform.into(),
            device_secret: key32(&cfg.device_secret, "device_secret")?,
            history_key: key32(&cfg.history_key, "history_key")?,
            force_relay: cfg.force_relay,
            allow_insecure_http: cfg.allow_insecure_http,
            mdns: !cfg.force_relay,
        };
        let svc = FsyncService::new(sc, Arc::new(Bridge(delegate)))?;
        Ok(Arc::new(Self { svc }))
    }

    pub fn start(&self) -> FfiResult<()> {
        unit(self.svc.start())
    }
    pub fn stop(&self) -> FfiResult<()> {
        unit(self.svc.stop())
    }
    pub fn node_id(&self) -> String {
        self.svc.node_id()
    }

    pub fn login_start(&self, email: String) -> FfiResult<()> {
        unit(self.svc.login_start(&email))
    }
    pub fn login_verify(&self, email: String, code: String) -> FfiResult<SessionTokens> {
        map(self.svc.login_verify(&email, &code))
    }
    pub fn set_session(&self, tokens: SessionTokens) -> FfiResult<()> {
        unit(self.svc.set_session(tokens.into()))
    }
    pub fn logout(&self) -> FfiResult<()> {
        unit(self.svc.logout())
    }
    pub fn ensure_account(&self) -> FfiResult<AccountState> {
        map(self.svc.ensure_account())
    }
    /// Re-creates the account's root with this device only. `ReauthRequired`: re-run the OTP.
    pub fn reset_account_trust(&self) -> FfiResult<AccountState> {
        map(self.svc.reset_account_trust())
    }
    pub fn pending_trust_resets(&self) -> Vec<TrustResetInfo> {
        self.svc
            .pending_trust_resets()
            .into_iter()
            .map(TrustResetInfo::from)
            .collect()
    }
    pub fn confirm_trust_reset(&self, reset_id: String) -> FfiResult<AccountState> {
        map(self.svc.confirm_trust_reset(&reset_id))
    }
    pub fn decline_trust_reset(&self, reset_id: String) -> FfiResult<()> {
        unit(self.svc.decline_trust_reset(&reset_id))
    }

    pub fn pairing_offer(&self, mode: PairingMode) -> FfiResult<PairingOffer> {
        map(self.svc.pairing_offer(mode.into()))
    }
    pub fn pairing_join(&self, input: String) -> FfiResult<PairingPeer> {
        map(self.svc.pairing_join(&input))
    }
    pub fn pairing_confirm(&self, request_id: u64, accept: bool) -> FfiResult<()> {
        unit(self.svc.pairing_confirm(request_id, accept))
    }
    pub fn pairing_cancel(&self) -> FfiResult<()> {
        unit(self.svc.pairing_cancel())
    }

    pub fn devices(&self) -> Vec<Device> {
        self.svc.devices().into_iter().map(Device::from).collect()
    }
    pub fn remove_device(&self, node_id: String) -> FfiResult<()> {
        unit(self.svc.remove_device(&node_id))
    }

    pub fn publish_clip(&self, clip: LocalClip) -> FfiResult<Published> {
        map(self.svc.publish_clip(clip.into()))
    }
    pub fn fetch_rep(&self, item_id: String, idx: u32) -> FfiResult<Vec<u8>> {
        self.svc.fetch_rep(&item_id, idx).map_err(FsyncError::from)
    }
    pub fn fetch_file_to(&self, item_id: String, file_id: u32, dest_dir: String) -> FfiResult<u64> {
        self.svc
            .fetch_file_to(&item_id, file_id, &dest_dir)
            .map_err(FsyncError::from)
    }
    pub fn read_file_range(
        &self,
        item_id: String,
        file_id: u32,
        offset: u64,
        len: u32,
    ) -> FfiResult<Vec<u8>> {
        self.svc
            .read_file_range(&item_id, file_id, offset, len)
            .map_err(FsyncError::from)
    }
    pub fn send_files(&self, node_id: String, paths: Vec<String>) -> FfiResult<u64> {
        self.svc
            .send_files(&node_id, paths)
            .map_err(FsyncError::from)
    }
    pub fn cancel(&self, transfer_id: u64) -> FfiResult<()> {
        unit(self.svc.cancel(transfer_id))
    }

    pub fn history(
        &self,
        query: Option<String>,
        cursor: Option<String>,
        limit: u32,
    ) -> FfiResult<HistoryPage> {
        map(self.svc.history(query, cursor, limit))
    }
    pub fn pin(&self, item_id: String, pinned: bool) -> FfiResult<()> {
        unit(self.svc.pin(&item_id, pinned))
    }
    pub fn delete(&self, item_id: String) -> FfiResult<()> {
        unit(self.svc.delete(&item_id))
    }
    pub fn clear(&self, keep_pinned: bool) -> FfiResult<()> {
        unit(self.svc.clear(keep_pinned))
    }
    pub fn settings(&self) -> FfiResult<Settings> {
        map(self.svc.settings())
    }
    pub fn set_settings(&self, settings: Settings) -> FfiResult<()> {
        unit(self.svc.set_settings(svc::Settings::from(settings)))
    }
}
