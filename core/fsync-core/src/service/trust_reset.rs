//! Trust reset (ADR 012): re-create the account's root from this device, and answer a reset
//! another device made.

use super::{
    FsyncService,
    clips::now,
    convert::{parse_reset_id, reset_id},
    error::{FsyncError, SvcResult},
    inner::{Inner, lock},
    types::{AccountState, TrustResetInfo},
};
use crate::error::CoreError;
use fsync_proto::{DeviceList, NodeId, ProtoError, SignedDeviceList, sign, verify_genesis};
use std::sync::Arc;

impl Inner {
    pub(super) async fn reset_account_trust(self: &Arc<Self>) -> SvcResult<AccountState> {
        let account = (*lock(&self.account)?).ok_or(FsyncError::Auth)?;
        // Before the server is touched: a pin of another account can never be replaced.
        if self.trust.list().is_some_and(|l| l.account_id != account) {
            return Err(CoreError::from(ProtoError::WrongAccount).into());
        }
        // Our own reset must not show up as someone else's when the push for it arrives.
        let _own = self.trust.begin_own_reset()?;
        let Some(server_head) = self.api.get_device_list().await? else {
            return Err(FsyncError::invalid(
                "the account has no device list to reset; use ensure_account",
            ));
        };
        let genesis = if is_own_fresh_root(&server_head, account, &self.me()) {
            // A reset the server already took (pinning it failed last time): finish it
            // without spending another one of the daily resets.
            server_head
        } else {
            let genesis = DeviceList::genesis(
                account,
                self.me(),
                self.cfg.device_name.clone(),
                self.cfg.platform,
                now(),
            );
            let signed = sign(self.identity.signing_key(), &genesis).map_err(CoreError::from)?;
            // Step-up auth: a stale login fails here as `ReauthRequired` and nothing changes.
            self.api.put_trust_reset(&signed).await?;
            signed
        };
        self.trust.replace_with_genesis(&genesis).map_err(|e| {
            FsyncError::Internal(format!(
                "the server accepted the reset but this device could not pin it ({e}); \
                 call reset_account_trust again to finish it"
            ))
        })?;
        self.restart_presence()?;
        Ok(AccountState::Created)
    }

    pub(super) fn confirm_trust_reset(self: &Arc<Self>, id: &str) -> SvcResult<AccountState> {
        let head = parse_reset_id(id)?;
        // Unknown, already answered, or the offered chain grew since it was shown: `NotFound`.
        if self.trust.confirm_reset(&head)?.is_none() {
            return Err(FsyncError::NotFound);
        }
        // Pinning a list without this device already fired `SelfRemoved`. Drop the pin as
        // well: pairing refuses to run over an existing one, and the user has to re-pair.
        if self.trust.is_self_removed() {
            self.trust.clear()?;
        }
        // The pin is done: a presence failure must not turn into an error, a retry would find
        // nothing pending.
        if let Err(e) = self.restart_presence() {
            tracing::warn!("trust reset confirmed but presence did not restart: {e}");
        }
        Ok(if self.trust.is_trusted(&self.me()) {
            AccountState::Ready
        } else {
            AccountState::NeedsPairing
        })
    }

    fn decline_trust_reset(&self, id: &str) -> SvcResult<()> {
        if self.trust.decline_reset(&parse_reset_id(id)?)? {
            Ok(())
        } else {
            Err(FsyncError::NotFound)
        }
    }

    fn pending_trust_resets(&self) -> Vec<TrustResetInfo> {
        self.trust
            .pending_reset_infos()
            .into_iter()
            .map(|r| TrustResetInfo {
                reset_id: reset_id(&r.head_hash),
                devices: u32::try_from(r.device_names.len()).unwrap_or(u32::MAX),
                device_names: r.device_names,
            })
            .collect()
    }
}

/// The server's list is a seq-0 root of `account` that only `me` signed and lists.
fn is_own_fresh_root(head: &SignedDeviceList, account: uuid::Uuid, me: &NodeId) -> bool {
    verify_genesis(head).is_ok()
        && head
            .decode()
            .is_ok_and(|l| l.account_id == account && l.devices.len() == 1 && l.contains(me))
}

impl FsyncService {
    /// Recovery when no other device can pair this one: re-creates the account's device list
    /// with this device as the only member and pins it (`Created`). Needs a signed-in session
    /// and an existing list; no pin is required, but a pin must be of the same account
    /// (checked before the server is asked). Every other device sees a `TrustReset` and must
    /// confirm it (it then has to re-pair) or decline it. `ReauthRequired`: the login is too
    /// old, run `login_start` / `login_verify` and call again. If the server took the reset
    /// but pinning it here failed (`Internal`), calling again finishes it without a new reset.
    pub fn reset_account_trust(&self) -> SvcResult<AccountState> {
        self.run(self.inner.reset_account_trust())
    }

    /// Accepts the reset `reset_id` (from `AccountEvent::TrustReset` or `pending_trust_resets`);
    /// pins exactly the list that id names. `Ready` if this device is still listed;
    /// `NeedsPairing` if not: `SelfRemoved` fired, sync and serving stopped, and the old pin was
    /// dropped so the device can pair again. Unknown, answered or superseded ids (the offered
    /// list grew and was announced under a new id): `NotFound`.
    pub fn confirm_trust_reset(&self, reset_id: &str) -> SvcResult<AccountState> {
        self.rt_enter(|| self.inner.confirm_trust_reset(reset_id))
    }

    /// Refuses the reset: the current pin stays and the same root is not offered again.
    /// Unknown or stale ids: `NotFound`.
    pub fn decline_trust_reset(&self, reset_id: &str) -> SvcResult<()> {
        self.inner.decline_trust_reset(reset_id)
    }

    /// Resets waiting for an answer, for apps that missed the event.
    pub fn pending_trust_resets(&self) -> Vec<TrustResetInfo> {
        self.inner.pending_trust_resets()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;
    use fsync_proto::Platform;
    use uuid::Uuid;

    fn root(who: &Identity, account: Uuid) -> SignedDeviceList {
        let l = DeviceList::genesis(account, who.node_id(), "d".into(), Platform::Cli, 1);
        sign(who.signing_key(), &l).unwrap()
    }

    #[test]
    fn only_a_single_device_root_of_this_account_is_ours() {
        let (me, other, account) = (
            Identity::generate().unwrap(),
            Identity::generate().unwrap(),
            Uuid::new_v4(),
        );
        let mine = root(&me, account);
        assert!(is_own_fresh_root(&mine, account, &me.node_id()));
        assert!(
            !is_own_fresh_root(&mine, Uuid::new_v4(), &me.node_id()),
            "wrong account"
        );
        assert!(
            !is_own_fresh_root(&root(&other, account), account, &me.node_id()),
            "another device's root"
        );
        // A seq-1 list that still contains only me is not a fresh root; a two-device one neither.
        let two = DeviceList::next_add(
            &mine,
            me.node_id(),
            other.node_id(),
            "o".into(),
            Platform::Mac,
            2,
        )
        .unwrap();
        let two = sign(me.signing_key(), &two).unwrap();
        assert!(
            !is_own_fresh_root(&two, account, &me.node_id()),
            "multi-device"
        );
    }
}
