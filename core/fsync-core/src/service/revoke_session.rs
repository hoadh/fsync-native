//! `revoke_session`: revoke a refresh token without an engine (hard logout after a quit).
//!
//! `logout()` keeps a failed revocation in memory only. An app that quit before it settled
//! persists the token itself ("pending revoke") and calls this at its next launch. It reuses the
//! backend client and `ApiClient::revoke` (a 401 means the session is already dead: success).

use super::error::{FsyncError, SvcResult};
use crate::api::ApiClient;
use std::time::Duration;

/// Default bound of `revoke_session` (connect, TLS, request and response together).
pub const REVOKE_SESSION_BUDGET: Duration = Duration::from_secs(10);

/// Blocks until `refresh`'s whole session is revoked on `api_base`, or `budget` ran out.
/// `Ok`: revoked or already dead (401). `Network`: no answer in time, backend trouble or
/// throttling; keep the token and retry later. Other errors (`Invalid`: bad URL or a refusal
/// that a retry will not change) are final. Call it off the UI thread and outside any tokio
/// runtime (it runs its own).
pub fn revoke_session(
    api_base: &str,
    refresh: &str,
    allow_insecure_http: bool,
    budget: Duration,
) -> SvcResult<()> {
    if refresh.is_empty() {
        return Err(FsyncError::invalid("refresh token is empty"));
    }
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(FsyncError::Internal(
            "blocking service call made from inside an async runtime".into(),
        ));
    }
    let api = ApiClient::with_options(api_base, None, None, allow_insecure_http)?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| FsyncError::Internal(format!("runtime: {e}")))?;
    rt.block_on(async {
        match tokio::time::timeout(budget, api.revoke(refresh)).await {
            Ok(r) => r.map_err(FsyncError::from),
            Err(_) => Err(FsyncError::Network("backend did not answer in time".into())),
        }
    })
}
