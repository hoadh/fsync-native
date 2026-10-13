//! Chunk-group hash list: a flattened one-level blake3 outboard.
//!
//! A file is cut into 16 KiB groups. The non-root chaining value (CV) of each group, merged
//! with blake3's own tree rule, yields exactly `blake3::hash(file)`. A receiver that holds the
//! CV list therefore (1) checks the list against the announced hash, then (2) verifies every
//! group independently, in any order. Cost: 32 bytes per 16 KiB (0.2 %).

use blake3::hazmat::{HasherExt, Mode, merge_subtrees_non_root, merge_subtrees_root};
use fsync_proto::file::CHUNK_GROUP;
use std::io::Read;

pub type Cv = [u8; 32];
pub const GROUP: usize = CHUNK_GROUP as usize;

/// Number of groups for a file of `size` bytes.
pub fn group_count(size: u64) -> u64 {
    size.div_ceil(CHUNK_GROUP)
}

/// Non-root CV of the group starting at byte `offset` (a multiple of 16 KiB, data non-empty).
pub fn group_cv(data: &[u8], offset: u64) -> Cv {
    let mut h = blake3::Hasher::new();
    h.set_input_offset(offset);
    h.update(data);
    h.finalize_non_root()
}

/// Largest power of two strictly below `n` (n >= 2): blake3's left-subtree rule on groups.
fn split(n: usize) -> usize {
    1 << (usize::BITS - 1 - (n - 1).leading_zeros())
}

fn subtree(cvs: &[Cv]) -> Cv {
    match cvs {
        [one] => *one,
        _ => {
            let k = split(cvs.len());
            merge_subtrees_non_root(&subtree(&cvs[..k]), &subtree(&cvs[k..]), Mode::Hash)
        }
    }
}

/// Root hash from the CVs of >= 2 groups (`None` otherwise: a single group is hashed whole).
pub fn root_of(cvs: &[Cv]) -> Option<[u8; 32]> {
    if cvs.len() < 2 {
        return None;
    }
    let k = split(cvs.len());
    let root = merge_subtrees_root(&subtree(&cvs[..k]), &subtree(&cvs[k..]), Mode::Hash);
    Some(*root.as_bytes())
}

/// Hash + group CVs of a whole stream (`cvs` is empty for <= 1 group).
pub struct Digest {
    pub size: u64,
    pub hash: [u8; 32],
    pub cvs: Vec<Cv>,
}

fn fill(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

pub fn digest_read(mut r: impl Read) -> std::io::Result<Digest> {
    let mut buf = vec![0u8; GROUP];
    let (mut size, mut cvs) = (0u64, Vec::new());
    let mut first: Option<Vec<u8>> = None;
    loop {
        let n = fill(&mut r, &mut buf)?;
        if n == 0 {
            break;
        }
        if size == 0 {
            first = Some(buf[..n].to_vec()); // only used when it is the sole group
        }
        cvs.push(group_cv(&buf[..n], size));
        size += n as u64;
        if n < GROUP {
            break;
        }
    }
    let hash = match root_of(&cvs) {
        Some(h) => h,
        None => *blake3::hash(first.as_deref().unwrap_or_default()).as_bytes(),
    };
    if cvs.len() < 2 {
        cvs.clear();
    }
    Ok(Digest { size, hash, cvs })
}

/// Checks received data against the announced hash using the verified CV list.
#[derive(Clone)]
pub struct Verifier {
    pub hash: [u8; 32],
    pub size: u64,
    pub cvs: std::sync::Arc<Vec<Cv>>,
}

impl Verifier {
    /// The CV list must merge to the announced hash (when there are >= 2 groups).
    pub fn new(hash: [u8; 32], size: u64, cvs: Vec<Cv>) -> Option<Self> {
        let ok = match root_of(&cvs) {
            Some(r) => r == hash && cvs.len() as u64 == group_count(size),
            None => cvs.is_empty() && group_count(size) <= 1,
        };
        ok.then(|| Self {
            hash,
            size,
            cvs: cvs.into(),
        })
    }

    /// `data` starts at group-aligned `pos`; every group but the file's last must be full.
    pub fn check(&self, pos: u64, data: &[u8]) -> bool {
        if !pos.is_multiple_of(CHUNK_GROUP)
            || pos
                .checked_add(data.len() as u64)
                .is_none_or(|e| e > self.size)
        {
            return false;
        }
        if group_count(self.size) <= 1 {
            return pos == 0
                && data.len() as u64 == self.size
                && blake3::hash(data).as_bytes() == &self.hash;
        }
        let mut off = pos;
        data.chunks(GROUP).all(|g| {
            let last = off + g.len() as u64 == self.size;
            let ok = (g.len() == GROUP || last)
                && usize::try_from(off / CHUNK_GROUP)
                    .ok()
                    .and_then(|idx| self.cvs.get(idx))
                    .is_some_and(|cv| *cv == group_cv(g, off));
            off += g.len() as u64;
            ok
        })
    }
}
