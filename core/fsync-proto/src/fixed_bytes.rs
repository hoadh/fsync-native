//! Fixed-size byte arrays larger than serde's 32-element array limit.
//! Human-readable formats (JSON) use standard base64; binary formats (postcard) use raw bytes.

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FixedBytes<const N: usize>(pub [u8; N]);

/// ed25519 signature.
pub type Sig = FixedBytes<64>;
/// Server-issued presence challenge.
pub type Nonce = FixedBytes<32>;

impl<const N: usize> fmt::Debug for FixedBytes<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FixedBytes<{N}>({})", STANDARD.encode(self.0))
    }
}

impl<const N: usize> Serialize for FixedBytes<N> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            s.serialize_str(&STANDARD.encode(self.0))
        } else {
            s.serialize_bytes(&self.0)
        }
    }
}

struct BytesVisitor<const N: usize>;

impl<const N: usize> de::Visitor<'_> for BytesVisitor<N> {
    type Value = FixedBytes<N>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{N} bytes (raw or base64)")
    }
    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        <[u8; N]>::try_from(v)
            .map(FixedBytes)
            .map_err(|_| E::invalid_length(v.len(), &self))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        let raw = STANDARD.decode(v).map_err(E::custom)?;
        self.visit_bytes(&raw)
    }
}

impl<'de, const N: usize> Deserialize<'de> for FixedBytes<N> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if d.is_human_readable() {
            d.deserialize_str(BytesVisitor::<N>)
        } else {
            d.deserialize_bytes(BytesVisitor::<N>)
        }
    }
}

impl<const N: usize> From<[u8; N]> for FixedBytes<N> {
    fn from(v: [u8; N]) -> Self {
        Self(v)
    }
}
