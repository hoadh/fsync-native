//! Something the sender can serve over file/1: bytes in memory, a file on disk, or a blob.

use super::group_tree::{Cv, Digest, digest_read};
use crate::{
    blobs::{BlobStore, Hash},
    error::{CoreError, Result, TransferError},
    fs_util::pread,
};
use std::{fs::File, path::PathBuf, sync::Arc, time::UNIX_EPOCH};

pub enum Source {
    Mem(Arc<Vec<u8>>),
    /// Snapshot of a user file: size + mtime are re-checked before serving.
    Path {
        path: PathBuf,
        mtime_ms: u64,
    },
    Blob(Hash),
}

pub struct Served {
    pub source: Source,
    pub size: u64,
    pub hash: Hash,
    pub cvs: Vec<Cv>,
}

fn stat(path: &PathBuf) -> std::io::Result<(u64, u64)> {
    let m = std::fs::metadata(path)?;
    let mtime = m
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Ok((m.len(), mtime))
}

impl Served {
    pub fn from_digest(source: Source, d: Digest) -> Self {
        Self {
            source,
            size: d.size,
            hash: d.hash,
            cvs: d.cvs,
        }
    }

    pub fn memory(bytes: Arc<Vec<u8>>) -> Result<Self> {
        let d = digest_read(bytes.as_slice())?;
        Ok(Self::from_digest(Source::Mem(bytes), d))
    }

    /// Hash a file; fails with `Changed` if it was modified while being read.
    pub fn snapshot(path: PathBuf) -> Result<Self> {
        let before = stat(&path)?;
        let d = digest_read(File::open(&path)?)?;
        if stat(&path)? != before || d.size != before.0 {
            return Err(TransferError::Changed.into());
        }
        Ok(Self::from_digest(
            Source::Path {
                path,
                mtime_ms: before.1,
            },
            d,
        ))
    }

    /// False when a path source differs from the announced snapshot (or vanished).
    pub fn unchanged(&self) -> bool {
        match &self.source {
            Source::Path { path, mtime_ms } => {
                stat(path).is_ok_and(|s| s == (self.size, *mtime_ms))
            }
            _ => true,
        }
    }

    pub fn read_at(&self, store: &dyn BlobStore, offset: u64, buf: &mut [u8]) -> Result<usize> {
        match &self.source {
            Source::Mem(b) => {
                let start = usize::try_from(offset).map_err(|_| CoreError::Invalid("offset"))?;
                let src = b.get(start..).unwrap_or_default();
                let n = src.len().min(buf.len());
                buf[..n].copy_from_slice(&src[..n]);
                Ok(n)
            }
            Source::Path { path, .. } => Ok(pread(&File::open(path)?, buf, offset)?),
            Source::Blob(h) => store.read_at(h, offset, buf),
        }
    }
}
