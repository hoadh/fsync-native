//! Construction parameters of `FsyncService` (the FFI `EngineConfig` maps onto this).

use super::error::{FsyncError, SvcResult};
use fsync_proto::Platform;
use std::path::PathBuf;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct ServiceConfig {
    /// Holds `history.db` and `blobs/` (created 0700).
    pub data_dir: PathBuf,
    pub api_base: String,
    /// Our relay (none: LAN only).
    pub relay_url: Option<String>,
    pub device_name: String,
    pub platform: Platform,
    /// Device identity seed (ed25519), kept by the app in Keychain / DPAPI.
    pub device_secret: Zeroizing<[u8; 32]>,
    /// History + blob encryption key, kept by the app next to the device secret.
    pub history_key: Zeroizing<[u8; 32]>,
    /// Test flag: no direct paths, everything over the relay.
    pub force_relay: bool,
    /// Dev/test backends on plain http.
    pub allow_insecure_http: bool,
    pub mdns: bool,
}

// Never print key material.
impl std::fmt::Debug for ServiceConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceConfig")
            .field("data_dir", &self.data_dir)
            .field("api_base", &self.api_base)
            .field("device_name", &self.device_name)
            .finish_non_exhaustive()
    }
}

/// Exactly 32 bytes or `Invalid` (FFI hands keys over as byte vectors).
pub fn key32(raw: &[u8], what: &str) -> SvcResult<Zeroizing<[u8; 32]>> {
    <[u8; 32]>::try_from(raw)
        .map(Zeroizing::new)
        .map_err(|_| FsyncError::Invalid(format!("{what} must be 32 bytes")))
}
