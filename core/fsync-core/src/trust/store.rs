//! Persistence of the pinned device-list head. Phase 08 adds a SQLCipher implementation.

use crate::{
    error::{CoreError, Result},
    fs_util::write_private_atomic,
};
use fsync_proto::SignedDeviceList;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Mutex};

/// Highest verified list seen, plus the hash of the genesis (root) of its chain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedState {
    pub head: SignedDeviceList,
    pub root_hash: [u8; 32],
}

pub trait TrustStore: Send + Sync {
    fn load(&self) -> Result<Option<PinnedState>>;
    fn save(&self, state: &PinnedState) -> Result<()>;
    fn clear(&self) -> Result<()>;
}

/// Single file, atomic replace, mode 0600.
pub struct FileTrustStore {
    path: PathBuf,
}

impl FileTrustStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl TrustStore for FileTrustStore {
    fn load(&self) -> Result<Option<PinnedState>> {
        match std::fs::read(&self.path) {
            Ok(raw) => postcard::from_bytes(&raw)
                .map(Some)
                .map_err(|e| CoreError::Store(format!("corrupt trust file: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn save(&self, state: &PinnedState) -> Result<()> {
        write_private_atomic(&self.path, &postcard::to_stdvec(state)?)
    }

    fn clear(&self) -> Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

/// Volatile store (tests, ephemeral tools).
#[derive(Default)]
pub struct MemoryTrustStore(Mutex<Option<PinnedState>>);

impl TrustStore for MemoryTrustStore {
    fn load(&self) -> Result<Option<PinnedState>> {
        Ok(self.0.lock().map_err(poisoned)?.clone())
    }
    fn save(&self, state: &PinnedState) -> Result<()> {
        *self.0.lock().map_err(poisoned)? = Some(state.clone());
        Ok(())
    }
    fn clear(&self) -> Result<()> {
        *self.0.lock().map_err(poisoned)? = None;
        Ok(())
    }
}

fn poisoned<T>(_: std::sync::PoisonError<T>) -> CoreError {
    CoreError::Store("lock poisoned".into())
}
