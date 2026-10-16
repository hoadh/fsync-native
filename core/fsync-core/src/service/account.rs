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
    /// own once the backend is back. Any other failure, or a device with nothing pinned, is an error.
    fn offline_fallback(self: &Arc<Self>, e: CoreError) -> SvcResult<AccountState> {
        let unreachable = matches!(e, CoreError::Net(_) | CoreError::Http(_));
        if unreachable && self.trust.list().is_some() && self.trust.is_trusted(&self.me()) {
            tracing::warn!("backend unreachable, using the cached device list: {e}");
            self.restart_presence()?;
            return Ok(AccountState::Ready);
        }
        Err(e.into())
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
