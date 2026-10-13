//! Download into the blob store with resume from the last verified group.
//!
//! Invariant: a `.part` file only ever holds bytes that passed group verification, written
//! sequentially. On resume the kept prefix is re-verified against the fresh CV list (a crash
//! may have left garbage in blocks the filesystem had not flushed) before trusting it.

use super::fetch_client::{Expect, Session, next_batch, open};
use super::group_tree::{GROUP, Verifier};
use super::policy::fetch_deadline;
use crate::{
    blobs::{BlobStore, blocking},
    error::{CoreError, Result, TransferError},
    net::Dialer,
};
use fsync_proto::{FetchReq, FetchTarget, NodeId, file::CHUNK_GROUP};
use std::sync::Arc;
use uuid::Uuid;

const BATCH: u64 = 1 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FetchStats {
    pub size: u64,
    /// Bytes already present and verified locally when the transfer started.
    pub resumed_from: u64,
    /// Bytes pulled over the network (file data only).
    pub bytes_received: u64,
}

/// Fetch `target` from `peer` into `store` (content-addressed by `exp.hash`).
pub async fn fetch_to_store(
    dialer: &Dialer,
    store: &Arc<dyn BlobStore>,
    peer: &NodeId,
    item_id: Uuid,
    target: FetchTarget,
    exp: Expect,
) -> Result<FetchStats> {
    let s = store.clone();
    if blocking(move || s.size_of(&exp.hash)).await? == Some(exp.size) {
        return Ok(FetchStats {
            size: exp.size,
            resumed_from: exp.size,
            bytes_received: 0,
        });
    }
    if exp.size == 0 {
        if blake3::hash(b"").as_bytes() != &exp.hash {
            return Err(TransferError::Integrity.into());
        }
        let s = store.clone();
        blocking(move || s.put(&exp.hash, &[])).await?;
        return Ok(FetchStats {
            size: 0,
            resumed_from: 0,
            bytes_received: 0,
        });
    }
    // Overall budget on top of the per-read stall timeout; progress is kept for a later resume.
    tokio::time::timeout(fetch_deadline(exp.size), async {
        // One retry: the kept prefix may fail re-verification and get cut back.
        for _ in 0..2 {
            match attempt(dialer, store, peer, item_id, target, exp).await {
                Err(CoreError::Invalid("restart")) => continue,
                r => return r,
            }
        }
        Err(CoreError::Invalid("could not resume transfer"))
    })
    .await
    .map_err(|_| CoreError::Transfer(TransferError::Timeout))?
}

async fn attempt(
    dialer: &Dialer,
    store: &Arc<dyn BlobStore>,
    peer: &NodeId,
    item_id: Uuid,
    target: FetchTarget,
    exp: Expect,
) -> Result<FetchStats> {
    let s = store.clone();
    let have = blocking(move || {
        let len = s.part_len(&exp.hash)?;
        let keep = if len > exp.size {
            0
        } else {
            len / CHUNK_GROUP * CHUNK_GROUP
        };
        if keep != len {
            s.truncate_part(&exp.hash, keep)?;
        }
        Ok(keep)
    })
    .await?;
    let req = FetchReq::new(item_id, target, have);
    let Session { mut rx, verifier } = open(dialer, peer, req, exp, None).await?;
    if have > 0
        && let Some(bad) = first_bad_group(store, &verifier, have).await?
    {
        let s = store.clone();
        blocking(move || s.truncate_part(&exp.hash, bad)).await?;
        let _ = rx.stop(0u32.into());
        return Err(CoreError::Invalid("restart"));
    }
    let mut pos = have;
    while pos < exp.size {
        let want = BATCH.min(exp.size - pos);
        let batch = next_batch(&mut rx, &verifier, pos, want as usize).await?;
        let s = store.clone();
        blocking(move || s.append_part(&exp.hash, pos, &batch)).await?;
        pos += want;
    }
    let s = store.clone();
    blocking(move || s.finish(&exp.hash, exp.size)).await?;
    Ok(FetchStats {
        size: exp.size,
        resumed_from: have,
        bytes_received: exp.size - have,
    })
}

/// Start offset of the first kept group that fails verification, if any.
async fn first_bad_group(
    store: &Arc<dyn BlobStore>,
    v: &Arc<Verifier>,
    kept: u64,
) -> Result<Option<u64>> {
    let (store, v) = (store.clone(), v.clone());
    blocking(move || {
        let mut buf = vec![0u8; BATCH as usize];
        let mut pos = 0u64;
        while pos < kept {
            let n = usize::try_from(BATCH.min(kept - pos)).unwrap_or(GROUP);
            let mut got = 0;
            while got < n {
                match store.read_part_at(&v.hash, pos + got as u64, &mut buf[got..n])? {
                    0 => break,
                    k => got += k,
                }
            }
            if got < n || !v.check(pos, &buf[..n]) {
                // Re-check group by group to find the exact cut point.
                return Ok(Some(cut_point(&v, pos, &buf[..got])));
            }
            pos += n as u64;
        }
        Ok(None)
    })
    .await
}

fn cut_point(v: &Verifier, pos: u64, data: &[u8]) -> u64 {
    let mut off = pos;
    for g in data.chunks(GROUP) {
        if !v.check(off, g) {
            break;
        }
        off += g.len() as u64;
    }
    off
}
