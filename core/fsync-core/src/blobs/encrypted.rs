//! `BlobStore` wrapper that keeps complete blobs encrypted at rest (XChaCha20-Poly1305 STREAM).
//!
//! Content addressing stays on the plaintext blake3 hash. Partial downloads are staged in the
//! inner store as plaintext under a keyed, unlinkable id and sealed chunk by chunk on `finish`
//! (memory bounded by one 64 KiB segment); `sweep_stale_parts` removes abandoned stagings.

use super::{
    BlobInfo, BlobStore, Hash,
    encrypted_format::{
        CHUNK, CT_CHUNK, HEADER_LEN, PREFIX_LEN, Sealer, Src, TAG, chunk_count, fill, header,
        parse_header, plain_len, read_exact,
    },
};
use crate::error::{CoreError, Result};
use hkdf::Hkdf;
use sha2::Sha256;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use zeroize::Zeroizing;

pub struct EncryptedBlobStore {
    inner: Arc<dyn BlobStore>,
    aead_key: Zeroizing<[u8; 32]>,
    id_key: Zeroizing<[u8; 32]>,
    /// `seal_all` stages through the inner `<hash>.part`: one at a time.
    seal_lock: Mutex<()>,
}

impl EncryptedBlobStore {
    /// Subkeys are derived from the 32-byte history key (never used directly).
    pub fn new(inner: Arc<dyn BlobStore>, history_key: &[u8; 32]) -> Result<Self> {
        let hk = Hkdf::<Sha256>::new(Some(b"fsync-blobs-v1"), history_key);
        let mut s = Self {
            inner,
            aead_key: Zeroizing::new([0; 32]),
            id_key: Zeroizing::new([0; 32]),
            seal_lock: Mutex::new(()),
        };
        hk.expand(b"blob aead key", s.aead_key.as_mut())
            .and_then(|_| hk.expand(b"staging id key", s.id_key.as_mut()))
            .map_err(|_| CoreError::Store("key derivation failed".into()))?;
        Ok(s)
    }

    fn staging_id(&self, h: &Hash) -> Hash {
        *blake3::keyed_hash(&self.id_key, h).as_bytes()
    }

    fn sealer(&self, h: &Hash) -> Result<Sealer> {
        let mut hdr = [0u8; HEADER_LEN as usize];
        read_exact(self.inner.as_ref(), h, 0, &mut hdr)?;
        Ok(Sealer::new(&self.aead_key, &parse_header(&hdr)?, *h))
    }

    fn stored_len(&self, h: &Hash) -> Result<u64> {
        self.inner
            .size_of(h)?
            .ok_or(CoreError::Io(std::io::ErrorKind::NotFound.into()))
    }

    /// Seal everything `src` yields into the inner store as the complete blob `h`.
    fn seal_all(&self, h: &Hash, src: &mut Src<'_>) -> Result<()> {
        let _g = self.seal_lock.lock().map_err(|_| CoreError::Closed)?;
        self.inner.truncate_part(h, 0)?;
        let mut prefix = [0u8; PREFIX_LEN];
        getrandom::fill(&mut prefix).map_err(|e| CoreError::Store(format!("rng: {e}")))?;
        let sealer = Sealer::new(&self.aead_key, &prefix, *h);
        let hdr = header(&prefix);
        self.inner.append_part(h, 0, &hdr)?;
        let mut off = hdr.len() as u64;
        let mut cur = Zeroizing::new(vec![0u8; CHUNK]);
        let mut next = Zeroizing::new(vec![0u8; CHUNK]);
        let mut n = fill(src, &mut cur)?;
        let mut idx = 0u32;
        loop {
            let nn = if n == CHUNK { fill(src, &mut next)? } else { 0 };
            let last = nn == 0;
            let mut out = Zeroizing::new(Vec::with_capacity(CHUNK + TAG));
            out.extend_from_slice(&cur[..n]);
            sealer.seal(idx, last, &mut out)?;
            self.inner.append_part(h, off, &out)?;
            off += out.len() as u64;
            if last {
                break;
            }
            std::mem::swap(&mut cur, &mut next);
            n = nn;
            idx = idx
                .checked_add(1)
                .ok_or(CoreError::Invalid("blob too large"))?;
        }
        self.inner.finish(h, off)
    }
}

impl BlobStore for EncryptedBlobStore {
    fn size_of(&self, h: &Hash) -> Result<Option<u64>> {
        self.inner.size_of(h)?.map(plain_len).transpose()
    }

    fn read_at(&self, h: &Hash, offset: u64, buf: &mut [u8]) -> Result<usize> {
        let stored = self.stored_len(h)?;
        let plain = plain_len(stored)?;
        if offset >= plain || buf.is_empty() {
            return Ok(0);
        }
        let sealer = self.sealer(h)?;
        let want = buf.len().min((plain - offset) as usize);
        let nchunks = chunk_count(plain);
        let mut ct = Zeroizing::new(Vec::with_capacity(CT_CHUNK as usize));
        let mut done = 0;
        while done < want {
            let pos = offset + done as u64;
            let (i, within) = (pos / CHUNK as u64, (pos % CHUNK as u64) as usize);
            let last = i + 1 == nchunks;
            let ct_off = HEADER_LEN + i * CT_CHUNK;
            let ct_len = if last { stored - ct_off } else { CT_CHUNK } as usize;
            ct.resize(ct_len, 0);
            read_exact(self.inner.as_ref(), h, ct_off, &mut ct)?;
            let idx = u32::try_from(i).map_err(|_| CoreError::Invalid("blob too large"))?;
            sealer.open(idx, last, &mut ct)?;
            let take = (ct.len() - within).min(want - done);
            buf[done..done + take].copy_from_slice(&ct[within..within + take]);
            done += take;
        }
        Ok(done)
    }

    fn read_part_at(&self, h: &Hash, offset: u64, buf: &mut [u8]) -> Result<usize> {
        self.inner.read_part_at(&self.staging_id(h), offset, buf)
    }

    fn put(&self, h: &Hash, data: &[u8]) -> Result<()> {
        if self.inner.size_of(h)?.is_some() {
            return Ok(());
        }
        let mut rest = data;
        self.seal_all(h, &mut |buf| {
            let n = buf.len().min(rest.len());
            buf[..n].copy_from_slice(&rest[..n]);
            rest = &rest[n..];
            Ok(n)
        })
    }

    fn part_len(&self, h: &Hash) -> Result<u64> {
        self.inner.part_len(&self.staging_id(h))
    }
    fn append_part(&self, h: &Hash, offset: u64, data: &[u8]) -> Result<()> {
        self.inner.append_part(&self.staging_id(h), offset, data)
    }
    fn truncate_part(&self, h: &Hash, len: u64) -> Result<()> {
        self.inner.truncate_part(&self.staging_id(h), len)
    }
    fn finish(&self, h: &Hash, expected_size: u64) -> Result<()> {
        let pid = self.staging_id(h);
        if self.inner.size_of(h)?.is_none() {
            // As in the plain store: the staged partial must exist and be `expected_size` long.
            self.inner.read_part_at(&pid, 0, &mut [])?;
            let plen = self.inner.part_len(&pid)?;
            if plen != expected_size {
                return Err(CoreError::Invalid("partial blob length mismatch"));
            }
            let mut off = 0u64;
            self.seal_all(h, &mut |buf| {
                let n = buf.len().min((plen - off) as usize);
                if n == 0 {
                    return Ok(0);
                }
                match self.inner.read_part_at(&pid, off, &mut buf[..n])? {
                    0 => Err(CoreError::Store("staged blob truncated".into())),
                    got => {
                        off += got as u64;
                        Ok(got)
                    }
                }
            })?;
        }
        self.inner.truncate_part(&pid, 0)
    }

    fn remove(&self, h: &Hash) -> Result<()> {
        self.inner.remove(h)?;
        self.inner.remove(&self.staging_id(h))
    }
    fn free_space(&self) -> Result<Option<u64>> {
        self.inner.free_space()
    }
    fn list(&self) -> Result<Vec<BlobInfo>> {
        self.inner.list()
    }
    fn sweep_stale_parts(&self, older_than: Duration) -> Result<usize> {
        self.inner.sweep_stale_parts(older_than)
    }
}
