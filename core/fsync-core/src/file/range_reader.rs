//! `read_file_range`: random + sequential reads of a remote file without storing all of it.
//!
//! Data arrives in 4 MiB windows, each verified group by group. A caller that reads one small
//! chunk at a time (the Windows shell: 256 KiB, never two in flight) would pay one round trip
//! per window, so once two reads in a row are sequential the next `AHEAD` windows are fetched
//! concurrently (ADR 010). A file already complete in the blob store is read locally.

use super::fetch_client::{Expect, Session, next_batch, open};
use super::group_tree::Verifier;
use crate::{
    blobs::blocking,
    engine::Shared,
    error::{CoreError, Result},
};
use fsync_proto::{FetchReq, FetchTarget, NodeId};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::OnceCell;
use uuid::Uuid;

const WINDOW: u64 = 4 << 20;
const MAX_WINDOWS: usize = 8;
/// Windows kept in flight ahead of a sequential reader (16 MiB: 50 MiB/s at ~300 ms RTT).
const AHEAD: u64 = 4;
/// Sequential reads seen before the full read-ahead depth is used; until then one window.
const RAMP_READS: u32 = 2;

type Cell = Arc<OnceCell<Arc<Vec<u8>>>>;

#[derive(Default)]
struct State {
    verifier: Option<Arc<Verifier>>,
    windows: HashMap<u64, Cell>,
    last_end: u64,
    /// Consecutive sequential reads.
    run: u32,
}

pub struct RangeReader {
    sh: Arc<Shared>,
    from: NodeId,
    item: Uuid,
    file_id: u32,
    exp: Expect,
    state: Mutex<State>,
    /// In-flight prefetches; dropping the set (on `shutdown`) aborts them.
    prefetch: Mutex<tokio::task::JoinSet<()>>,
}

impl RangeReader {
    pub fn new(sh: Arc<Shared>, from: NodeId, item: Uuid, file_id: u32, exp: Expect) -> Arc<Self> {
        Arc::new(Self {
            sh,
            from,
            item,
            file_id,
            exp,
            state: Mutex::default(),
            prefetch: Mutex::default(),
        })
    }

    /// Up to `len` bytes at `offset` (shorter at EOF, empty past it).
    pub async fn read(self: &Arc<Self>, offset: u64, len: usize) -> Result<Vec<u8>> {
        let end = self.exp.size.min(offset.saturating_add(len as u64));
        if offset >= end {
            return Ok(vec![]);
        }
        if let Some(b) = self.read_local(offset, end).await? {
            return Ok(b);
        }
        let mut out = Vec::with_capacity((end - offset) as usize);
        for idx in offset / WINDOW..=(end - 1) / WINDOW {
            let w = self.window(idx).await?;
            let start = (offset.max(idx * WINDOW) - idx * WINDOW) as usize;
            let stop = ((end - idx * WINDOW) as usize).min(w.len());
            out.extend_from_slice(w.get(start..stop).unwrap_or_default());
        }
        self.prefetch_after(offset, end);
        Ok(out)
    }

    async fn read_local(&self, offset: u64, end: u64) -> Result<Option<Vec<u8>>> {
        let (store, h, size) = (self.sh.store.clone(), self.exp.hash, self.exp.size);
        blocking(move || {
            if store.size_of(&h)? != Some(size) {
                return Ok(None);
            }
            let mut buf = vec![0u8; (end - offset) as usize];
            let mut got = 0;
            while got < buf.len() {
                match store.read_at(&h, offset + got as u64, &mut buf[got..])? {
                    0 => break,
                    n => got += n,
                }
            }
            buf.truncate(got);
            Ok(Some(buf))
        })
        .await
    }

    fn prefetch_after(self: &Arc<Self>, offset: u64, end: u64) {
        let first = (end - 1) / WINDOW + 1;
        let missing: Vec<u64> = {
            let Ok(mut s) = self.state.lock() else {
                return;
            };
            if offset == s.last_end || offset == 0 {
                s.run = s.run.saturating_add(1);
            } else {
                s.run = 0;
            }
            s.last_end = end;
            if s.run == 0 {
                return;
            }
            // Behind a sequential reader nothing is read again: free it for the windows ahead.
            let keep_from = offset / WINDOW;
            s.windows.retain(|k, _| *k >= keep_from);
            let depth = if s.run >= RAMP_READS { AHEAD } else { 1 };
            (first..first + depth)
                .filter(|i| i * WINDOW < self.exp.size && !s.windows.contains_key(i))
                .collect()
        };
        let Ok(mut set) = self.prefetch.lock() else {
            return;
        };
        while set.try_join_next().is_some() {}
        for idx in missing {
            let me = self.clone();
            set.spawn(async move {
                let _ = me.window(idx).await;
            });
        }
    }

    /// Stop background work; the reader is being evicted.
    pub fn shutdown(&self) {
        if let Ok(mut set) = self.prefetch.lock() {
            set.abort_all();
        }
    }

    async fn window(&self, idx: u64) -> Result<Arc<Vec<u8>>> {
        let cell = {
            let mut s = self.state.lock().map_err(|_| CoreError::Closed)?;
            if s.windows.len() >= MAX_WINDOWS && !s.windows.contains_key(&idx) {
                let far = s.windows.keys().copied().max_by_key(|k| k.abs_diff(idx));
                far.map(|k| s.windows.remove(&k));
            }
            s.windows.entry(idx).or_default().clone()
        };
        cell.get_or_try_init(|| async { self.fetch_window(idx).await.map(Arc::new) })
            .await
            .cloned()
    }

    async fn fetch_window(&self, idx: u64) -> Result<Vec<u8>> {
        let off = idx * WINDOW;
        let len = WINDOW.min(self.exp.size - off);
        let known = self
            .state
            .lock()
            .map_err(|_| CoreError::Closed)?
            .verifier
            .clone();
        let req = FetchReq::new(self.item, FetchTarget::File(self.file_id), off)
            .ranged(len, known.is_some());
        let dialer = self.sh.dialer()?;
        let Session { mut rx, verifier } = open(dialer, &self.from, req, self.exp, known).await?;
        if let Ok(mut s) = self.state.lock() {
            s.verifier.get_or_insert(verifier.clone());
        }
        let data = next_batch(&mut rx, &verifier, off, len as usize).await?;
        let _ = rx.stop(0u32.into());
        Ok(data)
    }
}
