//! On-disk format of an encrypted blob: `"FSB1" | nonce prefix (19) | chunk*`.
//!
//! Chunks are the aead STREAM-BE32 construction over XChaCha20-Poly1305: plaintext segments of
//! 64 KiB, nonce = prefix || counter (u32 BE) || last-flag, AAD = content hash. Every chunk but
//! the last is full, so plaintext offsets map to chunk indexes (random access) and a truncated
//! file fails authentication because its final chunk is not sealed as "last".

use crate::{
    blobs::{BlobStore, Hash},
    error::{CoreError, Result},
};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305,
    aead::{
        generic_array::GenericArray,
        stream::{NewStream, StreamBE32, StreamPrimitive},
    },
};

pub const MAGIC: &[u8; 4] = b"FSB1";
pub const PREFIX_LEN: usize = 19;
pub const HEADER_LEN: u64 = (MAGIC.len() + PREFIX_LEN) as u64;
pub const CHUNK: usize = 64 * 1024;
pub const TAG: usize = 16;
pub const CT_CHUNK: u64 = (CHUNK + TAG) as u64;

fn bad(msg: &str) -> CoreError {
    CoreError::Store(msg.to_owned())
}

pub fn header(prefix: &[u8; PREFIX_LEN]) -> Vec<u8> {
    [MAGIC.as_slice(), prefix].concat()
}

pub fn parse_header(raw: &[u8]) -> Result<[u8; PREFIX_LEN]> {
    match raw.split_at_checked(MAGIC.len()) {
        Some((m, p)) if m == MAGIC => p.try_into().map_err(|_| bad("short blob header")),
        _ => Err(bad("not an encrypted blob")),
    }
}

/// Plaintext length of a blob stored in `stored` bytes.
pub fn plain_len(stored: u64) -> Result<u64> {
    let ct = stored
        .checked_sub(HEADER_LEN)
        .filter(|c| *c >= TAG as u64)
        .ok_or_else(|| bad("encrypted blob too short"))?;
    Ok(match ct % CT_CHUNK {
        0 => ct / CT_CHUNK * CHUNK as u64,
        r => ct / CT_CHUNK * CHUNK as u64 + r - TAG as u64,
    })
}

/// Number of chunks of a blob with `plain` bytes (an empty blob has one empty chunk).
pub fn chunk_count(plain: u64) -> u64 {
    plain.div_ceil(CHUNK as u64).max(1)
}

pub struct Sealer {
    prim: StreamBE32<XChaCha20Poly1305>,
    aad: Hash,
}

impl Sealer {
    pub fn new(key: &[u8; 32], prefix: &[u8; PREFIX_LEN], aad: Hash) -> Self {
        let aead = XChaCha20Poly1305::new(GenericArray::from_slice(key));
        let prim = StreamBE32::from_aead(aead, GenericArray::from_slice(prefix));
        Self { prim, aad }
    }

    /// Encrypts `buf` (plaintext) in place, appending the tag.
    pub fn seal(&self, idx: u32, last: bool, buf: &mut Vec<u8>) -> Result<()> {
        self.prim
            .encrypt_in_place(idx, last, &self.aad, buf)
            .map_err(|_| bad("encryption failed"))
    }

    /// Decrypts `buf` (ciphertext + tag) in place; fails on any modification.
    pub fn open(&self, idx: u32, last: bool, buf: &mut Vec<u8>) -> Result<()> {
        self.prim
            .decrypt_in_place(idx, last, &self.aad, buf)
            .map_err(|_| bad("blob authentication failed (tampered or wrong key)"))
    }
}

/// Sequential plaintext source for `seal_all`.
pub type Src<'a> = dyn FnMut(&mut [u8]) -> Result<usize> + 'a;

/// Fill `buf` from `src` until full or exhausted.
pub fn fill(src: &mut Src<'_>, buf: &mut [u8]) -> Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        match src(&mut buf[got..])? {
            0 => break,
            n => got += n,
        }
    }
    Ok(got)
}

pub fn read_exact(s: &dyn BlobStore, h: &Hash, off: u64, buf: &mut [u8]) -> Result<()> {
    let mut got = 0;
    while got < buf.len() {
        match s.read_at(h, off + got as u64, &mut buf[got..])? {
            0 => return Err(CoreError::Store("encrypted blob truncated".into())),
            n => got += n,
        }
    }
    Ok(())
}
