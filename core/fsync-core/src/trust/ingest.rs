//! Pinning and verification entry points.

use super::manager::{Ingest, TrustManager, UNKNOWN_ROOT};
use crate::{
    error::{CoreError, Result},
    events::EngineEvent,
};
use fsync_proto::{
    NodeId, Outcome, ProtoError, SignedDeviceList, device_list::DEVLIST_DOMAIN, sig::verify_domain,
    verify_chain, verify_chain_from_genesis, verify_genesis,
};

impl TrustManager {
    /// First device of an account: pin own genesis. Errors if something is already pinned.
    pub fn pin_genesis(&self, genesis: &SignedDeviceList) -> Result<()> {
        let _w = self.write_gate()?;
        if self.lock()?.pinned.is_some() {
            return Err(CoreError::Invalid("a device list is already pinned"));
        }
        verify_genesis(genesis)?;
        self.pin(genesis.clone(), genesis.hash()?)?;
        Ok(())
    }

    /// New device after pairing: verify the full chain from genesis and require it to list
    /// both `me` and `peer` (the device the user just confirmed).
    pub fn pin_paired_chain(
        &self,
        chain: &[SignedDeviceList],
        me: &NodeId,
        peer: &NodeId,
    ) -> Result<u64> {
        let _w = self.write_gate()?;
        if self.lock()?.pinned.is_some() {
            return Err(CoreError::Invalid("a device list is already pinned"));
        }
        let list = verify_chain_from_genesis(chain)?;
        if !list.contains(me) || !list.contains(peer) {
            return Err(ProtoError::SignerNotTrusted.into());
        }
        let (Some(head), Some(first)) = (chain.last(), chain.first()) else {
            return Err(ProtoError::InvalidGenesis("empty chain").into());
        };
        self.pin(head.clone(), first.hash()?)
    }

    /// New device after pairing, without the chain: pin `head` if `peer` signed it, it lists
    /// both `me` and `peer`, and (when known) it belongs to `account`. The root is unknown
    /// (`UNKNOWN_ROOT`), so a later seq-0 list prompts a reset unless the real chain shows up.
    pub fn pin_paired_head(
        &self,
        head: &SignedDeviceList,
        me: &NodeId,
        peer: &NodeId,
        account: Option<uuid::Uuid>,
    ) -> Result<u64> {
        let _w = self.write_gate()?;
        if self.lock()?.pinned.is_some() {
            return Err(CoreError::Invalid("a device list is already pinned"));
        }
        let list = head.decode()?;
        list.check_shape()?;
        verify_domain(&list.signer, DEVLIST_DOMAIN, &head.body, &head.sig)?;
        if &list.signer != peer {
            return Err(ProtoError::SignerNotTrusted.into());
        }
        if account.is_some_and(|a| a != list.account_id) {
            return Err(ProtoError::WrongAccount.into());
        }
        if !list.contains(me) || !list.contains(peer) {
            return Err(ProtoError::SignerNotTrusted.into());
        }
        self.pin(head.clone(), UNKNOWN_ROOT)
    }

    pub fn ingest(&self, signed: &SignedDeviceList) -> Result<Ingest> {
        self.ingest_chain(std::slice::from_ref(signed))
    }

    /// Apply consecutive lists (ascending). Entries older than the pin are ignored.
    pub fn ingest_chain(&self, chain: &[SignedDeviceList]) -> Result<Ingest> {
        let _w = self.write_gate()?;
        let mut g = self.lock()?;
        let Some(p) = g.pinned.as_ref() else {
            return Err(CoreError::Invalid("no pinned list; pair first"));
        };
        let (head, mut root) = (p.state.head.clone(), p.state.root_hash);
        let pinned_seq = p.list.seq;
        // Pinned via head only: a chain from genesis that runs through our head reveals the root.
        if root == UNKNOWN_ROOT
            && let Some(first) = chain.first()
            && first.decode()?.seq == 0
            && chain.contains(&head)
            && verify_chain_from_genesis(chain).is_ok()
        {
            root = first.hash()?;
        }
        // A seq-0 start that is not our root is a reset candidate, whatever our own seq is.
        if let Some(first) = chain.first()
            && first.decode()?.seq == 0
            && first.hash()? != root
        {
            return self.pend_reset(&mut g, chain);
        }
        let mut rel = Vec::with_capacity(chain.len());
        for s in chain {
            if s.decode()?.seq >= pinned_seq {
                rel.push(s.clone());
            }
        }
        match verify_chain(&head, &rel) {
            Ok(Outcome::Unchanged) => Ok(Ingest::Unchanged),
            Ok(Outcome::Advanced(l)) => {
                let new_head = rel.last().cloned().ok_or(ProtoError::EmptyList)?;
                drop(g);
                self.pin(new_head, root)?;
                self.events.emit(EngineEvent::ListAdvanced { seq: l.seq });
                Ok(Ingest::Advanced { seq: l.seq })
            }
            Err(e) => Err(e.into()),
        }
    }
}
