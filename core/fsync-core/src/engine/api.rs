//! Public `Engine` operations: publish clips, pull reps and files, ranged reads.

use super::{Engine, Shared};
use crate::{
    blobs::blocking,
    clip::{ClipInput, PublishOpts, PublishOutcome, ReceivedOffer},
    error::{CoreError, Result, TransferError},
    events::OfferRef,
    file::{Expect, FetchStats, RangeReader, check_fetch, fetch_to_store},
};
use fsync_proto::{FetchTarget, NodeId};
use std::sync::Arc;
use uuid::Uuid;

impl Shared {
    pub(super) fn offer_of(&self, id: &Uuid) -> Result<Arc<ReceivedOffer>> {
        self.log
            .lock()
            .map_err(|_| CoreError::Closed)?
            .get(id)
            .ok_or(CoreError::Transfer(TransferError::NotFound))
    }
}

impl Engine {
    /// Announce a clipboard item (or files) to paired devices; see `PublishOpts`.
    pub async fn publish_clip(
        &self,
        input: ClipInput,
        opts: PublishOpts,
    ) -> Result<PublishOutcome> {
        self.sh.publish(input, opts).await
    }

    /// A previously received (and validated) offer, while it is still remembered.
    pub fn received_offer(&self, item_id: &Uuid) -> Option<OfferRef> {
        self.sh
            .offer_of(item_id)
            .ok()
            .map(|r| OfferRef(r.offer.clone()))
    }

    /// Low-level pull of `target` from `peer` into the blob store, resuming a partial download.
    pub async fn fetch_blob_from(
        &self,
        peer: &NodeId,
        item_id: Uuid,
        target: FetchTarget,
        size: u64,
        hash: [u8; 32],
    ) -> Result<FetchStats> {
        let d = self.sh.dialer()?;
        check_fetch(&self.sh.store, hash, size, self.sh.cfg.max_file_bytes).await?;
        // Single-flight: a second fetch of the same content waits, then finds it complete.
        let _one = self.sh.fetch_locks.lock(&hash).await;
        fetch_to_store(
            d,
            &self.sh.store,
            peer,
            item_id,
            target,
            Expect { size, hash },
        )
        .await
    }

    /// Bytes of representation `rep` (inline ones return without network).
    pub async fn fetch_rep(&self, item_id: Uuid, rep: u32) -> Result<Vec<u8>> {
        let r = self.sh.offer_of(&item_id)?;
        let i = rep as usize;
        let meta = r
            .offer
            .reps
            .get(i)
            .ok_or(CoreError::Invalid("no such rep"))?;
        if let Some(Some(b)) = r.offer.inline.get(i) {
            return Ok(b.clone());
        }
        let max = self.sh.max_clip_bytes();
        if meta.size > max {
            return Err(CoreError::TooLarge {
                size: meta.size,
                max,
            });
        }
        self.fetch_blob_from(
            &r.from,
            item_id,
            FetchTarget::Rep(rep),
            meta.size,
            meta.hash,
        )
        .await?;
        self.sh.note_rep_fetched(item_id, rep).await;
        let (store, h, size) = (self.sh.store.clone(), meta.hash, meta.size as usize);
        let bytes = blocking(move || {
            let mut buf = vec![0u8; size];
            let mut got = 0;
            while got < size {
                match store.read_at(&h, got as u64, &mut buf[got..])? {
                    0 => return Err(CoreError::Invalid("blob shorter than expected")),
                    n => got += n,
                }
            }
            Ok(buf)
        })
        .await?;
        if r.offer.sensitive {
            self.drop_blob(meta.hash).await;
        }
        Ok(bytes)
    }

    /// Pull a file into the blob store (resumable).
    pub async fn fetch_file(&self, item_id: Uuid, file_id: u32) -> Result<FetchStats> {
        let r = self.sh.offer_of(&item_id)?;
        let f = r
            .offer
            .files
            .iter()
            .find(|f| f.file_id == file_id)
            .ok_or(CoreError::Invalid("no such file"))?;
        self.fetch_blob_from(&r.from, item_id, FetchTarget::File(file_id), f.size, f.hash)
            .await
    }

    /// Up to `len` bytes of a remote file at `offset`, verified; 30 s default deadline.
    /// Sequential reads prefetch the next window. For callers on async tasks.
    pub async fn read_file_range(
        &self,
        item_id: Uuid,
        file_id: u32,
        offset: u64,
        len: usize,
    ) -> Result<Vec<u8>> {
        let reader = self.reader(item_id, file_id)?;
        tokio::time::timeout(self.sh.cfg.read_timeout, reader.read(offset, len))
            .await
            .map_err(|_| CoreError::Transfer(TransferError::Timeout))?
    }

    /// Same, for plain threads (Windows `IStream::Read`). Must not run on a runtime worker.
    pub fn read_file_range_blocking(
        &self,
        item_id: Uuid,
        file_id: u32,
        offset: u64,
        len: usize,
    ) -> Result<Vec<u8>> {
        self.rt
            .block_on(self.read_file_range(item_id, file_id, offset, len))
    }

    /// Open ranged-read sessions currently cached (bounded; for diagnostics and tests).
    #[doc(hidden)]
    pub fn open_readers(&self) -> usize {
        self.sh.readers.lock().map_or(0, |g| g.len())
    }

    fn reader(&self, item: Uuid, file_id: u32) -> Result<Arc<RangeReader>> {
        let r = self.sh.offer_of(&item)?;
        let f = r
            .offer
            .files
            .iter()
            .find(|f| f.file_id == file_id)
            .ok_or(CoreError::Invalid("no such file"))?;
        if let Some(max) = self.sh.cfg.max_file_bytes
            && f.size > max
        {
            return Err(CoreError::TooLarge { size: f.size, max });
        }
        let mut g = self.sh.readers.lock().map_err(|_| CoreError::Closed)?;
        let exp = Expect {
            size: f.size,
            hash: f.hash,
        };
        let budget = g.budget(r.from);
        Ok(g.get_or_insert_with((item, file_id), || {
            RangeReader::new(self.sh.clone(), r.from, item, file_id, exp, budget)
        }))
    }
}
