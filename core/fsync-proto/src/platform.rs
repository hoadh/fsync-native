//! Device platform tag carried in signed device lists and pairing frames.
//!
//! Forward-compatible on the wire: postcard encodes the tag as a varint variant index. Older
//! builds used a derived enum and rejected any index they did not know, which would make a
//! whole hash chain undecodable for them. This codec keeps known variants byte-identical to
//! the old derive (Mac=0, Win=1, Cli=2), decodes Linux=3, and keeps any other index as
//! `Unknown(n)` that encodes back to the same index. That round trip matters because
//! `DeviceList::successor` decodes the previous list and re-encodes its entries: a tolerant
//! client must not silently rewrite a newer peer's platform.
//!
//! Emitting `Linux` is gated (ADR 014, "Platform emit gate"): no shipping build sends it until
//! the backend and the Mac/Windows apps carry this decoder.

use serde::de::{self, Deserializer, EnumAccess, VariantAccess, Visitor};
use serde::{Deserialize, Serialize, Serializer};
use std::fmt;

const NAME: &str = "Platform";
const VARIANTS: &[&str] = &["Mac", "Win", "Cli", "Linux"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Platform {
    Mac,
    Win,
    Cli,
    Linux,
    /// A platform index this build does not know; re-encoded unchanged.
    Unknown(u32),
}

impl Platform {
    pub fn index(self) -> u32 {
        match self {
            Self::Mac => 0,
            Self::Win => 1,
            Self::Cli => 2,
            Self::Linux => 3,
            Self::Unknown(n) => n,
        }
    }

    pub fn from_index(n: u32) -> Self {
        match n {
            0 => Self::Mac,
            1 => Self::Win,
            2 => Self::Cli,
            3 => Self::Linux,
            n => Self::Unknown(n),
        }
    }

    /// The closest platform older interfaces (FFI v1, UI icons) understand: Linux and unknown
    /// platforms show as `Cli`.
    pub fn wire_compat(self) -> Self {
        match self {
            Self::Linux | Self::Unknown(_) => Self::Cli,
            p => p,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Unknown(_) => "Unknown",
            p => VARIANTS[p.index() as usize],
        }
    }
}

impl Serialize for Platform {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_unit_variant(NAME, self.index(), self.name())
    }
}

/// Variant tag: an index (postcard and other compact formats) or a name (self-describing ones).
struct Tag(Platform);

impl<'de> Deserialize<'de> for Tag {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct TagVisitor;
        impl Visitor<'_> for TagVisitor {
            type Value = Tag;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a platform index or name")
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Tag, E> {
                u32::try_from(v)
                    .map(|n| Tag(Platform::from_index(n)))
                    .map_err(|_| E::custom("platform index out of range"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Tag, E> {
                match VARIANTS.iter().position(|n| *n == v) {
                    Some(i) => Ok(Tag(Platform::from_index(i as u32))),
                    None => Err(E::unknown_variant(v, VARIANTS)),
                }
            }
        }
        d.deserialize_identifier(TagVisitor)
    }
}

impl<'de> Deserialize<'de> for Platform {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct PlatformVisitor;
        impl<'de> Visitor<'de> for PlatformVisitor {
            type Value = Platform;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a Platform variant")
            }
            fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<Platform, A::Error> {
                let (Tag(p), variant) = data.variant::<Tag>()?;
                // Every variant, known or not, is a unit variant: no payload follows the index.
                variant.unit_variant()?;
                Ok(p)
            }
        }
        d.deserialize_enum(NAME, VARIANTS, PlatformVisitor)
    }
}
