//! First-device setup: create the genesis device list, or detect that pairing is needed.

use super::{
    FsyncService,
    clips::now,
    error::{FsyncError, SvcResult},
    inner::{Inner, lock},
    types::AccountState,
};
use crate::{error::CoreError, sync::sync_device_list};
use fsync_proto::{DeviceList, sign};
use std::sync::Arc;

impl Inner {
    /// The backend cannot be reached: a device that already pinned a list that includes it keeps
    /// working from that list (paired peers connect without the backend). Presence retries on its
    /// own once the backend is back. Fail-closed: only a plain unreachable error qualifies (not
    /// TLS, bad bodies or HTTP statuses), only while the list was confirmed within the last
    /// `MAX_AGE_SECS`, and never for a device with nothing pinned.
    fn offline_fallback(self: &Arc<Self>, e: CoreError) -> SvcResult<AccountState> {
        if self.trust.list().is_none() || !self.trust.is_trusted(&self.me()) {
            return Err(e.into());
        }
        let now = super::list_freshness::unix_now();
        match self.list_freshness.offline_ok(&e, now) {
            Some(age) => {
                let days = age / 86_400;
                tracing::warn!(
                    "backend unreachable, using the cached device list (last confirmed {days} d ago): {e}"
                );
                self.restart_presence()?;
                Ok(AccountState::Ready)
            }
            None if matches!(e, CoreError::Unreachable(_)) => {
                tracing::warn!(
                    "stale device list: not confirmed for over 7 days, refusing to start offline: {e}"
                );
                Err(e.into())
            }
            None => Err(e.into()),
        }
    }

    pub(super) async fn ensure_account(self: &Arc<Self>) -> SvcResult<AccountState> {
        let account = (*lock(&self.account)?).ok_or(FsyncError::Auth)?;
        // A lost genesis race (409) falls through to the "list exists" path on the 2nd round.
        for _ in 0..2 {
            let head = match self.api.get_device_list().await {
                Ok(h) => h,
                Err(e) => return self.offline_fallback(e),
            };
            let Some(_head) = head else {
                let genesis = DeviceList::genesis(
                    account,
                    self.me(),
                    self.cfg.device_name.clone(),
                    self.cfg.platform,
                    now(),
                );
                let signed =
                    sign(self.identity.signing_key(), &genesis).map_err(CoreError::from)?;
                match self.api.put_device_list(&signed, None).await {
                    Ok(_) => {
                        self.trust.pin_genesis(&signed)?;
                        self.list_freshness.stamp();
                        self.restart_presence()?;
                        return Ok(AccountState::Created);
                    }
                    Err(CoreError::Api(e)) if e.status == 409 => continue,
                    Err(e) => return Err(e.into()),
                }
            };
            let Some(pinned) = self.trust.list() else {
                return Ok(AccountState::NeedsPairing);
            };
            if pinned.account_id != account {
                return Err(FsyncError::invalid(
                    "this device is paired to another account",
                ));
            }
            if let Err(e) = sync_device_list(&self.api, &self.trust).await {
                return self.offline_fallback(e);
            }
            if !self.trust.is_trusted(&self.me()) {
                return Ok(AccountState::NeedsPairing);
            }
            self.restart_presence()?;
            return Ok(AccountState::Ready);
        }
        Err(FsyncError::invalid("device list is changing; try again"))
    }
}

impl FsyncService {
    /// After sign-in: first device creates and pins the account's genesis list (`Created`);
    /// a pinned device syncs (`Ready`); anything else must pair (`NeedsPairing`).
    pub fn ensure_account(&self) -> SvcResult<AccountState> {
        self.run(self.inner.ensure_account())
    }
}
