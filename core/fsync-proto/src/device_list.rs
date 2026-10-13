//! Signed device list: the account's trust root (postcard-canonical, hash-chained).

use crate::{
    error::ProtoError,
    fixed_bytes::Sig,
    node_id::NodeId,
    sig::{sign_domain, verify_domain},
};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const DEVLIST_DOMAIN: &[u8] = b"fsync-devlist-v1";
pub const DEVLIST_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Platform {
    Mac,
    Win,
    Cli,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceEntry {
    pub node_id: NodeId,
    pub name: String,
    pub platform: Platform,
    pub added_seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceList {
    pub v: u8,
    pub account_id: Uuid,
    pub seq: u64,
    /// blake3 of the previous `SignedDeviceList` postcard bytes; zero at genesis.
    pub prev_hash: [u8; 32],
    pub devices: Vec<DeviceEntry>,
    pub signer: NodeId,
    pub created_ms: u64,
}

/// Carries the exact signed bytes, so verification never depends on re-encoding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedDeviceList {
    pub body: Vec<u8>,
    pub sig: Sig,
}

impl DeviceList {
    /// Seq-0 list containing only the signer's own device.
    pub fn genesis(
        account_id: Uuid,
        signer: NodeId,
        name: String,
        platform: Platform,
        created_ms: u64,
    ) -> Self {
        Self {
            v: DEVLIST_VERSION,
            account_id,
            seq: 0,
            prev_hash: [0; 32],
            devices: vec![DeviceEntry {
                node_id: signer,
                name,
                platform,
                added_seq: 0,
            }],
            signer,
            created_ms,
        }
    }

    fn successor(
        prev: &SignedDeviceList,
        signer: NodeId,
        created_ms: u64,
    ) -> Result<Self, ProtoError> {
        let p = prev.decode()?;
        Ok(Self {
            seq: p.seq.checked_add(1).ok_or(ProtoError::SeqOverflow)?,
            prev_hash: prev.hash()?,
            signer,
            created_ms,
            ..p
        })
    }

    /// Next list (seq+1, prev_hash set) with `entry` appended.
    pub fn next_add(
        prev: &SignedDeviceList,
        signer: NodeId,
        node_id: NodeId,
        name: String,
        platform: Platform,
        created_ms: u64,
    ) -> Result<Self, ProtoError> {
        let mut l = Self::successor(prev, signer, created_ms)?;
        if l.contains(&node_id) {
            return Err(ProtoError::DuplicateDevice);
        }
        l.devices.push(DeviceEntry {
            node_id,
            name,
            platform,
            added_seq: l.seq,
        });
        l.check_shape()?;
        Ok(l)
    }

    /// Next list with `node_id` removed. Removing the last device is rejected (re-genesis instead).
    pub fn next_remove(
        prev: &SignedDeviceList,
        signer: NodeId,
        node_id: &NodeId,
        created_ms: u64,
    ) -> Result<Self, ProtoError> {
        let mut l = Self::successor(prev, signer, created_ms)?;
        let before = l.devices.len();
        l.devices.retain(|d| d.node_id != *node_id);
        if l.devices.len() == before {
            return Err(ProtoError::DeviceNotFound);
        }
        if l.devices.is_empty() {
            return Err(ProtoError::EmptyList);
        }
        Ok(l)
    }

    pub fn contains(&self, node_id: &NodeId) -> bool {
        self.devices.iter().any(|d| d.node_id == *node_id)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, ProtoError> {
        Ok(postcard::to_stdvec(self)?)
    }
}

impl SignedDeviceList {
    /// Sign `list` with `key`; `key` must be the list's declared signer.
    pub fn sign(key: &SigningKey, list: &DeviceList) -> Result<Self, ProtoError> {
        if NodeId::from_signing_key(key) != list.signer {
            return Err(ProtoError::SignerMismatch);
        }
        let body = list.to_bytes()?;
        let sig = sign_domain(key, DEVLIST_DOMAIN, &body);
        Ok(Self { body, sig })
    }

    pub fn decode(&self) -> Result<DeviceList, ProtoError> {
        let (list, rest) = postcard::take_from_bytes::<DeviceList>(&self.body)?;
        if !rest.is_empty() {
            return Err(ProtoError::TrailingBytes);
        }
        Ok(list)
    }

    /// Check the signature of `body` against `signer` (no trust decision).
    pub(crate) fn check_sig(&self, signer: &NodeId) -> Result<(), ProtoError> {
        verify_domain(signer, DEVLIST_DOMAIN, &self.body, &self.sig)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, ProtoError> {
        Ok(postcard::to_stdvec(self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProtoError> {
        let (s, rest) = postcard::take_from_bytes::<Self>(bytes)?;
        if !rest.is_empty() {
            return Err(ProtoError::TrailingBytes);
        }
        Ok(s)
    }

    /// blake3(postcard(SignedDeviceList)).
    pub fn hash(&self) -> Result<[u8; 32], ProtoError> {
        Ok(*blake3::hash(&self.to_bytes()?).as_bytes())
    }
}

pub fn sign(key: &SigningKey, list: &DeviceList) -> Result<SignedDeviceList, ProtoError> {
    SignedDeviceList::sign(key, list)
}
