//! Device identity: one ed25519 key is both the iroh endpoint key and the list-signing key.

use crate::{
    error::{CoreError, Result},
    fs_util::write_private_atomic,
};
use ed25519_dalek::SigningKey;
use fsync_proto::NodeId;
use std::path::Path;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct Identity {
    key: SigningKey,
}

// Never print key material.
impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Identity({})", self.node_id())
    }
}

impl Identity {
    pub fn generate() -> Result<Self> {
        let mut seed = Zeroizing::new([0u8; 32]);
        getrandom::fill(seed.as_mut()).map_err(|e| CoreError::Net(format!("rng: {e}")))?;
        Ok(Self::from_seed(&seed))
    }

    /// Restore from the 32-byte seed the app keeps in Keychain/DPAPI.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self {
            key: SigningKey::from_bytes(seed),
        }
    }

    pub fn seed(&self) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(self.key.to_bytes())
    }

    pub fn signing_key(&self) -> &SigningKey {
        &self.key
    }

    pub fn node_id(&self) -> NodeId {
        NodeId::from_signing_key(&self.key)
    }

    pub fn iroh_secret(&self) -> iroh::SecretKey {
        iroh::SecretKey::from_bytes(&self.seed())
    }

    /// File-backed identity for the CLI (0600). Apps on mac/win use the OS keystore instead.
    pub fn load_or_create(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(raw) => {
                let raw = Zeroizing::new(raw);
                let seed = <[u8; 32]>::try_from(raw.as_slice())
                    .map_err(|_| CoreError::Invalid("identity file must be 32 bytes"))?;
                Ok(Self::from_seed(&seed))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let id = Self::generate()?;
                write_private_atomic(path, id.seed().as_ref())?;
                Ok(id)
            }
            Err(e) => Err(e.into()),
        }
    }
}
