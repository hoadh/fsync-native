//! Trust-reset handling: a new genesis for the same account needs explicit user confirmation.

use super::manager::{Ingest, Inner, MAX_PENDING, TrustManager};
use crate::{
    error::{CoreError, Result},
    events::EngineEvent,
};
use fsync_proto::{ProtoError, SignedDeviceList, verify_chain_from_genesis};

impl TrustManager {
    pub(super) fn pend_reset(&self, g: &mut Inner, chain: &[SignedDeviceList]) -> Result<Ingest> {
        let (Some(first), Some(head)) = (chain.first(), chain.last()) else {
            return Err(ProtoError::TrustReset.into());
        };
        let root_hash = first.hash()?;
        let list = verify_chain_from_genesis(chain)?;
        // A reset is only ever a new root of *our* account.
        if g.pinned
            .as_ref()
            .is_some_and(|p| p.list.account_id != list.account_id)
        {
            return Err(ProtoError::WrongAccount.into());
        }
        g.pending.retain(|(h, _)| *h != root_hash);
        g.pending.push((root_hash, head.clone()));
        if g.pending.len() > MAX_PENDING {
            g.pending.remove(0);
        }
        let first_prompt = !g.prompted.contains(&root_hash);
        if first_prompt {
            g.prompted.push(root_hash);
            if g.prompted.len() > MAX_PENDING {
                g.prompted.remove(0);
            }
        }
        if first_prompt {
            self.events.emit(EngineEvent::TrustReset {
                root_hash,
                devices: list.devices.len(),
            });
        }
        Ok(Ingest::ResetPending { root_hash })
    }

    /// The user explicitly accepted the new root.
    pub fn confirm_reset(&self, root_hash: &[u8; 32]) -> Result<u64> {
        let _w = self.write_gate()?;
        let head = {
            let mut g = self.lock()?;
            let i = g
                .pending
                .iter()
                .position(|(h, _)| h == root_hash)
                .ok_or(CoreError::Invalid("no such pending trust reset"))?;
            let head = g.pending.remove(i).1;
            g.pending.clear();
            head
        };
        self.pin(head, *root_hash)
    }
}
