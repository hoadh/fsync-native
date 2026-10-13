//! Device-list verification rules (phase-06). Pure functions; callers pin the result.

use crate::{
    device_list::{DeviceList, SignedDeviceList},
    error::ProtoError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Byte-identical to the pinned list: nothing to do.
    Unchanged,
    /// Valid successor; caller should pin the new list.
    Advanced(DeviceList),
}

/// A genesis list is self-signed, seq 0, zero prev_hash, and lists only its signer.
pub fn verify_genesis(signed: &SignedDeviceList) -> Result<DeviceList, ProtoError> {
    let list = signed.decode()?;
    list.check_shape()?;
    if list.seq != 0 {
        return Err(ProtoError::InvalidGenesis("seq must be 0"));
    }
    if list.prev_hash != [0; 32] {
        return Err(ProtoError::InvalidGenesis("prev_hash must be zero"));
    }
    if list.devices.len() != 1 || list.devices[0].node_id != list.signer {
        return Err(ProtoError::InvalidGenesis("must contain only the signer"));
    }
    signed.check_sig(&list.signer)?;
    Ok(list)
}

/// Verify `new` as the direct successor of the pinned (already trusted) list.
pub fn verify_next(
    new: &SignedDeviceList,
    pinned: &SignedDeviceList,
) -> Result<Outcome, ProtoError> {
    if new == pinned {
        return Ok(Outcome::Unchanged);
    }
    let pinned_list = pinned.decode()?;
    let list = new.decode()?;
    list.check_shape()?;
    if list.account_id != pinned_list.account_id {
        return Err(ProtoError::WrongAccount);
    }
    if list.seq == 0 {
        // A different, valid seq-0 root for the same account: the whole chain was reset.
        verify_genesis(new)?;
        return Err(ProtoError::TrustReset);
    }
    if list.seq <= pinned_list.seq {
        return Err(ProtoError::Rollback {
            got: list.seq,
            pinned: pinned_list.seq,
        });
    }
    let expected = pinned_list
        .seq
        .checked_add(1)
        .ok_or(ProtoError::SeqOverflow)?;
    if list.seq > expected {
        return Err(ProtoError::Gap {
            expected,
            got: list.seq,
        });
    }
    if !pinned_list.contains(&list.signer) {
        return Err(ProtoError::SignerNotTrusted);
    }
    new.check_sig(&list.signer)?;
    if list.prev_hash != pinned.hash()? {
        return Err(ProtoError::PrevHashMismatch);
    }
    Ok(Outcome::Advanced(list))
}

/// Apply consecutive lists on top of `pinned` (e.g. after a `Gap`). Replays of already-pinned
/// lists are skipped only if byte-identical to the current head.
pub fn verify_chain(
    pinned: &SignedDeviceList,
    chain: &[SignedDeviceList],
) -> Result<Outcome, ProtoError> {
    let mut head = pinned;
    let mut latest = Outcome::Unchanged;
    for item in chain {
        match verify_next(item, head)? {
            Outcome::Unchanged => {}
            adv @ Outcome::Advanced(_) => {
                latest = adv;
                head = item;
            }
        }
    }
    Ok(latest)
}

/// Verify a full chain starting at a genesis (used by a freshly paired device that has no pin).
/// Returns the latest list.
pub fn verify_chain_from_genesis(chain: &[SignedDeviceList]) -> Result<DeviceList, ProtoError> {
    let (first, rest) = chain
        .split_first()
        .ok_or(ProtoError::InvalidGenesis("empty chain"))?;
    let genesis = verify_genesis(first)?;
    match verify_chain(first, rest)? {
        Outcome::Unchanged => Ok(genesis),
        Outcome::Advanced(l) => Ok(l),
    }
}
