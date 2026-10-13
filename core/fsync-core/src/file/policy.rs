//! Receiver-side limits on peer-declared file sizes (a lying `size` must not exhaust memory,
//! disk or time).

use crate::{
    blobs::{BlobStore, Hash, blocking},
    error::{CoreError, Result},
};
use fsync_proto::file::{CHUNK_GROUP, CV_LEN};
use std::{sync::Arc, time::Duration};

/// Largest chaining-value list we buffer (bounds file size at `MAX_CV_BYTES/32 * 16 KiB`).
pub const MAX_CV_BYTES: u64 = 64 * 1024 * 1024;
/// Floor of the per-fetch deadline.
const MIN_DEADLINE: Duration = Duration::from_secs(120);
/// Slowest acceptable average throughput (bytes/s) used to scale the deadline with size.
const MIN_RATE: u64 = 256 * 1024;

/// Largest file whose CV list fits `MAX_CV_BYTES`.
pub fn max_verifiable_size() -> u64 {
    (MAX_CV_BYTES / CV_LEN as u64).saturating_mul(CHUNK_GROUP)
}

/// Overall time budget of one fetch attempt: a minimum plus time at `MIN_RATE`.
pub fn fetch_deadline(size: u64) -> Duration {
    MIN_DEADLINE + Duration::from_secs(size / MIN_RATE)
}

/// Pure size policy. `needed` = bytes still to write; `free` = free space if known.
pub fn check_declared_size(
    size: u64,
    needed: u64,
    max_file_bytes: Option<u64>,
    free: Option<u64>,
) -> Result<()> {
    let limit = [
        Some(max_verifiable_size()),
        max_file_bytes,
        free.map(|f| f.saturating_add(size - needed)),
    ]
    .into_iter()
    .flatten()
    .min();
    match limit {
        Some(max) if size > max => Err(CoreError::TooLarge { size, max }),
        _ => Ok(()),
    }
}

/// Check a fetch of `size` bytes against configured cap and free space (counting what a
/// partial download already holds). Complete blobs always pass.
pub async fn check_fetch(
    store: &Arc<dyn BlobStore>,
    hash: Hash,
    size: u64,
    max_file_bytes: Option<u64>,
) -> Result<()> {
    let s = store.clone();
    blocking(move || {
        if s.size_of(&hash)?.is_some() {
            return Ok(());
        }
        let have = s.part_len(&hash)?.min(size);
        check_declared_size(size, size - have, max_file_bytes, s.free_space()?)
    })
    .await
}
