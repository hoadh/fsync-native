//! Trust-reset handling: a new genesis for the same account needs explicit user confirmation.
//!
//! A reset is identified by the hash of the offered *head*: the id the user is shown names
//! exactly the list that gets pinned. If the offered chain grows, the id changes and the new
//! head is announced again, so an old answer can never pin something that was not shown.

use super::manager::{Ingest, Inner, MAX_DECLINED, TrustManager};
use crate::{error::Result, events::EngineEvent};
use fsync_proto::{ProtoError, SignedDeviceList, verify_chain_from_genesis, verify_genesis};

/// A reset waiting for an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingReset {
    /// Hash of the offered head; pass to `confirm_reset` / `decline_reset`.
    pub head_hash: [u8; 32],
    /// Display names of the devices in the offered list (chosen by whoever made the reset).
    pub device_names: Vec<String>,
}

/// While alive, this device's own reset is not treated as someone else's (see `begin_own_reset`).
/// Counted, so overlapping resets do not end each other's protection.
pub struct OwnReset<'a>(&'a TrustManager);

impl Drop for OwnReset<'_> {
    fn drop(&mut self) {
        if let Ok(mut g) = self.0.lock() {
            g.resetting = g.resetting.saturating_sub(1);
        }
    }
}

impl TrustManager {
    pub(super) fn pend_reset(&self, g: &mut Inner, chain: &[SignedDeviceList]) -> Result<Ingest> {
        let (Some(first), Some(head)) = (chain.first(), chain.last()) else {
            return Err(ProtoError::TrustReset.into());
        };
        let (root_hash, head_hash) = (first.hash()?, head.hash()?);
        let list = verify_chain_from_genesis(chain)?;
        // A reset is only ever a new root of *our* account.
        if g.pinned
            .as_ref()
            .is_some_and(|p| p.list.account_id != list.account_id)
        {
            return Err(ProtoError::WrongAccount.into());
        }
        if let Some(d) = g.declined.iter_mut().find(|(r, _)| *r == root_hash) {
            d.1 = head_hash; // remembered so the sync can skip refetching it
            return Ok(Ingest::Unchanged);
        }
        if g.resetting > 0 {
            return Ok(Ingest::Unchanged);
        }
        // An honest server serves one chain: a different root makes the earlier offers stale.
        if g.pending.first().is_some_and(|(r, _)| *r == root_hash) {
            if g.pending[0].1.hash()? == head_hash {
                return Ok(Ingest::ResetPending { head_hash }); // already announced
            }
            g.pending[0].1 = head.clone(); // the chain grew: announce the new head
        } else {
            g.pending.clear();
            g.pending.push((root_hash, head.clone()));
        }
        self.events.emit(EngineEvent::TrustReset {
            head_hash,
            device_names: list.devices.into_iter().map(|d| d.name).collect(),
        });
        Ok(Ingest::ResetPending { head_hash })
    }

    /// The user explicitly accepted the offered head `head_hash`; every pending entry is cleared
    /// once it is pinned. `Ok(None)`: no such pending reset (answered already, or the offered
    /// chain grew or was replaced since it was announced).
    pub fn confirm_reset(&self, head_hash: &[u8; 32]) -> Result<Option<u64>> {
        let _w = self.write_gate()?;
        let (root, head) = {
            let g = self.lock()?;
            let Some((root, head)) = g
                .pending
                .iter()
                .find(|(_, h)| h.hash().is_ok_and(|x| x == *head_hash))
            else {
                return Ok(None);
            };
            (*root, head.clone())
        };
        let seq = self.pin(head, root)?;
        self.lock()?.pending.clear();
        Ok(Some(seq))
    }

    /// The user refused the offered head: forget it and keep the current pin. The whole root is
    /// not offered again by later syncs, even when its chain grows. `Ok(false)`: not pending.
    pub fn decline_reset(&self, head_hash: &[u8; 32]) -> Result<bool> {
        let _w = self.write_gate()?;
        let mut g = self.lock()?;
        let Some(i) = g
            .pending
            .iter()
            .position(|(_, h)| h.hash().is_ok_and(|x| x == *head_hash))
        else {
            return Ok(false);
        };
        let (root, _) = g.pending.remove(i);
        g.declined.push((root, *head_hash));
        if g.declined.len() > MAX_DECLINED {
            g.declined.remove(0);
        }
        Ok(true)
    }

    /// True if `head_hash` is the last head seen of a root the user declined.
    pub fn is_declined_head(&self, head_hash: &[u8; 32]) -> bool {
        self.lock()
            .is_ok_and(|g| g.declined.iter().any(|(_, h)| h == head_hash))
    }

    /// Pending resets with the devices they offer, oldest first.
    pub fn pending_reset_infos(&self) -> Vec<PendingReset> {
        let Ok(g) = self.lock() else {
            return Vec::new();
        };
        g.pending
            .iter()
            .filter_map(|(_, head)| {
                let decoded = head.decode().and_then(|l| Ok((l, head.hash()?)));
                match decoded {
                    Ok((l, head_hash)) => Some(PendingReset {
                        head_hash,
                        device_names: l.devices.into_iter().map(|d| d.name).collect(),
                    }),
                    Err(e) => {
                        tracing::warn!("dropping an undecodable pending trust reset: {e}");
                        None
                    }
                }
            })
            .collect()
    }

    /// Call before the backend is asked to re-create the root, keep the guard until the new
    /// genesis is pinned: the device-list push that follows must not announce our own reset.
    pub fn begin_own_reset(&self) -> Result<OwnReset<'_>> {
        let mut g = self.lock()?;
        g.resetting = g.resetting.saturating_add(1);
        drop(g);
        Ok(OwnReset(self))
    }

    /// The signed-in user re-created the account's root (the backend already accepted it):
    /// replace whatever is pinned with `genesis` and drop all pending / declined resets.
    /// Works without an existing pin; with one, the account must be the same.
    pub fn replace_with_genesis(&self, genesis: &SignedDeviceList) -> Result<()> {
        let _w = self.write_gate()?;
        verify_genesis(genesis)?;
        let account = genesis.decode()?.account_id;
        if self
            .lock()?
            .pinned
            .as_ref()
            .is_some_and(|p| p.list.account_id != account)
        {
            return Err(ProtoError::WrongAccount.into());
        }
        self.pin(genesis.clone(), genesis.hash()?)?;
        let mut g = self.lock()?;
        g.pending.clear();
        g.declined.clear();
        Ok(())
    }
}
