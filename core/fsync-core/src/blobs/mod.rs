//! Content-addressed blob storage for received and spilled data.
//!
//! The trait is synchronous (call it from `spawn_blocking`); phase 08 wraps it to encrypt at
//! rest. Hashes are blake3 of the plaintext.

mod encrypted;
mod encrypted_format;
mod export;
mod fs_store;

pub use encrypted::EncryptedBlobStore;
pub use fs_store::FsBlobStore;

use crate::error::Result;
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

pub type Hash = [u8; 32];

/// A complete blob as it sits on disk (`size` is the stored size, ciphertext if encrypted).
#[derive(Clone, Copy, Debug)]
pub struct BlobInfo {
    pub hash: Hash,
    pub size: u64,
    pub modified: SystemTime,
}

pub trait BlobStore: Send + Sync + 'static {
    /// Size of the complete blob, if present.
    fn size_of(&self, hash: &Hash) -> Result<Option<u64>>;
    /// Positional read from a complete blob; returns bytes read (0 = EOF).
    fn read_at(&self, hash: &Hash, offset: u64, buf: &mut [u8]) -> Result<usize>;
    /// Positional read from the partial download (to re-verify it on resume).
    fn read_part_at(&self, hash: &Hash, offset: u64, buf: &mut [u8]) -> Result<usize>;
    /// Store a complete blob atomically. Caller guarantees `blake3(data) == hash`.
    fn put(&self, hash: &Hash, data: &[u8]) -> Result<()>;
    /// Length of the partial download (0 if none).
    fn part_len(&self, hash: &Hash) -> Result<u64>;
    /// Append verified bytes to the partial download; `offset` must equal its current length.
    fn append_part(&self, hash: &Hash, offset: u64, data: &[u8]) -> Result<()>;
    fn truncate_part(&self, hash: &Hash, len: u64) -> Result<()>;
    /// Promote the (fully verified) partial download to a complete blob. Contract: errors
    /// unless the partial exists and is exactly `expected_size` bytes; never conjures an empty
    /// blob out of a missing partial (an empty blob is stored with `put(hash, &[])`).
    fn finish(&self, hash: &Hash, expected_size: u64) -> Result<()>;
    fn remove(&self, hash: &Hash) -> Result<()>;
    /// Free bytes on the volume backing the store, if it can tell (used to refuse transfers
    /// that cannot fit).
    fn free_space(&self) -> Result<Option<u64>> {
        Ok(None)
    }
    /// All complete blobs (for retention / orphan GC).
    fn list(&self) -> Result<Vec<BlobInfo>>;
    /// Delete partial downloads untouched for `older_than`; returns how many.
    fn sweep_stale_parts(&self, older_than: Duration) -> Result<usize>;

    /// Write a complete blob next to `dest` without ever overwriting or following links:
    /// exclusive temp file, exclusive reservation of the final name (`name (1).ext` on
    /// collision), temp removed on error. Returns the path actually used.
    fn export(&self, hash: &Hash, dest: &Path) -> Result<PathBuf> {
        export::export_with(dest, |off, buf| self.read_at(hash, off, buf))
    }
}

/// Run blocking store work off the async threads.
pub async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|_| crate::error::CoreError::Closed)?
}
