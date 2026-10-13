//! Inner store that records the widest buffer crossing the boundary.
use fsync_core::{
    Result,
    blobs::{BlobInfo, BlobStore, FsBlobStore},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::Relaxed},
    },
    time::Duration,
};

pub struct Spy {
    pub inner: FsBlobStore,
    pub max: Arc<AtomicUsize>,
}

impl Spy {
    fn note(&self, n: usize) {
        self.max.fetch_max(n, Relaxed);
    }
}

impl BlobStore for Spy {
    fn size_of(&self, h: &[u8; 32]) -> Result<Option<u64>> {
        self.inner.size_of(h)
    }
    fn read_at(&self, h: &[u8; 32], o: u64, b: &mut [u8]) -> Result<usize> {
        self.note(b.len());
        self.inner.read_at(h, o, b)
    }
    fn read_part_at(&self, h: &[u8; 32], o: u64, b: &mut [u8]) -> Result<usize> {
        self.note(b.len());
        self.inner.read_part_at(h, o, b)
    }
    fn put(&self, h: &[u8; 32], d: &[u8]) -> Result<()> {
        self.inner.put(h, d)
    }
    fn part_len(&self, h: &[u8; 32]) -> Result<u64> {
        self.inner.part_len(h)
    }
    fn append_part(&self, h: &[u8; 32], o: u64, d: &[u8]) -> Result<()> {
        self.note(d.len());
        self.inner.append_part(h, o, d)
    }
    fn truncate_part(&self, h: &[u8; 32], l: u64) -> Result<()> {
        self.inner.truncate_part(h, l)
    }
    fn finish(&self, h: &[u8; 32], n: u64) -> Result<()> {
        self.inner.finish(h, n)
    }
    fn remove(&self, h: &[u8; 32]) -> Result<()> {
        self.inner.remove(h)
    }
    fn list(&self) -> Result<Vec<BlobInfo>> {
        self.inner.list()
    }
    fn sweep_stale_parts(&self, d: Duration) -> Result<usize> {
        self.inner.sweep_stale_parts(d)
    }
}
