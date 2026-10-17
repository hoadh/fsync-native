//! Full content of a history item, for copying it back to the clipboard.

use super::{
    FsyncService,
    convert::parse_uuid,
    error::{FsyncError, SvcResult},
    types::ClipRep,
};
use crate::{blobs::blocking, error::CoreError};

impl FsyncService {
    /// Every representation of a history item, in the order it was published, with its full
    /// bytes (`history()` only carries previews). Works without `start()` for reps that are
    /// stored; a received rep that was never fetched is pulled from the sender when the engine
    /// runs, and skipped when the sender is unreachable (offline, offer forgotten; later pulls
    /// are then not attempted). An item that yields nothing because of that is `NotFound`.
    /// A files-only item has no reps and returns an empty list. Unknown ids are `NotFound`,
    /// malformed ones `Invalid`.
    pub fn history_item_reps(&self, item_id: &str) -> SvcResult<Vec<ClipRep>> {
        let id = parse_uuid(item_id)?;
        let h = &self.inner.history;
        let detail = self
            .run(async { Ok(h.item(id).await?) })?
            .ok_or(FsyncError::NotFound)?;
        let mut out = Vec::with_capacity(detail.reps.len());
        // After one pull fails because the sender is unreachable, the others would stall too.
        let mut can_pull = true;
        for rep in &detail.reps {
            let (store, hash, size) = (self.inner.store.clone(), rep.hash, rep.size);
            let stored = self
                .run(async move { Ok(blocking(move || read_all(&*store, &hash, size)).await?) });
            let data = match stored {
                Ok(Some(d)) => Some(d),
                Ok(None) if can_pull => {
                    let pulled = self.pull_rep(id, rep.idx)?;
                    can_pull = pulled.is_some();
                    pulled
                }
                Ok(None) => None,
                Err(e) => return Err(e),
            };
            if let Some(data) = data {
                out.push(ClipRep {
                    mime: rep.mime.clone(),
                    data,
                });
            }
        }
        if out.is_empty() && !detail.reps.is_empty() {
            return Err(FsyncError::NotFound);
        }
        Ok(out)
    }

    /// Bytes of a rep that is not in the store, from the sender. `None` when it cannot be had
    /// right now (engine stopped, sender offline or gone); real failures (integrity, size, I/O)
    /// are errors, not a silently shorter item.
    fn pull_rep(&self, id: uuid::Uuid, idx: u32) -> SvcResult<Option<Vec<u8>>> {
        let Ok(engine) = self.inner.engine() else {
            return Ok(None);
        };
        match self.run(async move { Ok(engine.fetch_rep(id, idx).await?) }) {
            Ok(d) => Ok(Some(d)),
            Err(
                e @ (FsyncError::NotPaired
                | FsyncError::PeerRefused
                | FsyncError::PeerOffline
                | FsyncError::Network(_)
                | FsyncError::NotFound),
            ) => {
                tracing::debug!("history rep {idx} not available: {e}");
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}

/// `None` when the blob is not (completely) in the store.
fn read_all(
    store: &dyn crate::blobs::BlobStore,
    hash: &crate::blobs::Hash,
    size: u64,
) -> Result<Option<Vec<u8>>, CoreError> {
    if store.size_of(hash)? != Some(size) {
        return Ok(None);
    }
    let len = usize::try_from(size).map_err(|_| CoreError::Invalid("rep too large"))?;
    let mut buf = vec![0u8; len];
    let mut got = 0;
    while got < len {
        match store.read_at(hash, got as u64, &mut buf[got..])? {
            0 => return Err(CoreError::Invalid("blob shorter than expected")),
            n => got += n,
        }
    }
    Ok(Some(buf))
}
