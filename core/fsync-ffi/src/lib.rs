//! FFI surface (contract v1, see docs/ffi-contract.md). Thin adapters over
//! `fsync_core::service`: records and enums are mirrored, nothing else lives here.
//!
//! Methods block; call them off the UI thread. Delegate callbacks arrive on one dedicated
//! background thread (never a runtime worker), in order; marshal to the UI thread there.

mod convert;
mod delegate;
mod engine;
mod error;
mod types_account;
mod types_clip;
mod types_device;
mod types_history;

pub use delegate::EngineDelegate;
pub use engine::{EngineConfig, FsyncEngine};
pub use error::FsyncError;
pub use types_account::*;
pub use types_clip::*;
pub use types_device::*;
pub use types_history::*;

uniffi::setup_scaffolding!();

/// Bumped on every breaking contract change (v2: trust-reset API, `ReauthRequired`,
/// `AccountEvent::TrustReset`).
#[uniffi::export]
pub fn contract_version() -> u32 {
    2
}

/// Abandons the history in `data_dir` (moved aside as `history.corrupt-*`, not deleted) and
/// clears its key tag. Use after `FsyncEngine::new` failed with `HistoryKeyMismatch` and the user
/// confirmed. Also drops pinned trust stored in the history: the device must re-pair. Call only
/// while no engine uses `data_dir`.
#[uniffi::export]
pub fn reset_history(data_dir: String) -> Result<(), FsyncError> {
    fsync_core::service::FsyncService::reset_history(std::path::Path::new(&data_dir))
        .map_err(FsyncError::from)
}

/// Revokes a refresh token without an engine: the retry of a `logout` that was not confirmed
/// (see `FsyncEngine::logout_confirmed`), e.g. at the next launch. Blocking, bounded by about
/// 10 s: call it off the UI thread. `Ok` = revoked or already dead (401): delete the token.
/// `Network` = no answer in time or backend trouble: keep it and retry later. Other errors are
/// final. `allow_insecure_http` is for dev backends only.
#[uniffi::export]
pub fn revoke_session(
    api_url: String,
    refresh_token: String,
    allow_insecure_http: bool,
) -> Result<(), FsyncError> {
    fsync_core::service::revoke_session(
        &api_url,
        &refresh_token,
        allow_insecure_http,
        fsync_core::service::REVOKE_SESSION_BUDGET,
    )
    .map_err(FsyncError::from)
}
