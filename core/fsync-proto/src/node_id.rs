//! `NodeId`: the 32-byte ed25519 public key; identical bytes to iroh's NodeId/EndpointId.

use crate::error::ProtoError;
use data_encoding::{BASE32_NOPAD, HEXLOWER, HEXLOWER_PERMISSIVE};
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::{fmt, str::FromStr};

/// Display/FromStr format: 64 lowercase hex chars (same as iroh). Serde: hex string in
/// JSON, raw 32 bytes in postcard.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub [u8; 32]);

impl NodeId {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn from_signing_key(key: &SigningKey) -> Self {
        Self(key.verifying_key().to_bytes())
    }

    pub fn verifying_key(&self) -> Result<VerifyingKey, ProtoError> {
        VerifyingKey::from_bytes(&self.0).map_err(|_| ProtoError::InvalidKey)
    }

    /// Lowercase unpadded base32 (used in QR payloads to keep them short and case-safe).
    pub fn to_base32(&self) -> String {
        BASE32_NOPAD.encode(&self.0).to_lowercase()
    }

    pub fn from_base32(s: &str) -> Result<Self, ProtoError> {
        let raw = BASE32_NOPAD
            .decode(s.to_uppercase().as_bytes())
            .map_err(|_| ProtoError::InvalidNodeId)?;
        <[u8; 32]>::try_from(raw.as_slice())
            .map(Self)
            .map_err(|_| ProtoError::InvalidNodeId)
    }
}

impl From<&SigningKey> for NodeId {
    fn from(key: &SigningKey) -> Self {
        Self::from_signing_key(key)
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&HEXLOWER.encode(&self.0))
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({self})")
    }
}

impl FromStr for NodeId {
    type Err = ProtoError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw = HEXLOWER_PERMISSIVE
            .decode(s.as_bytes())
            .map_err(|_| ProtoError::InvalidNodeId)?;
        <[u8; 32]>::try_from(raw.as_slice())
            .map(Self)
            .map_err(|_| ProtoError::InvalidNodeId)
    }
}

impl Serialize for NodeId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            s.collect_str(self)
        } else {
            self.0.serialize(s)
        }
    }
}

impl<'de> Deserialize<'de> for NodeId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if d.is_human_readable() {
            let s = String::deserialize(d)?;
            s.parse().map_err(de::Error::custom)
        } else {
            <[u8; 32]>::deserialize(d).map(Self)
        }
    }
}
