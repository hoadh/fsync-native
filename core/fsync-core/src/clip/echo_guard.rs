//! Loop breaker: content just applied from a peer must not be re-broadcast when the local
//! clipboard watcher sees it change.

use fsync_proto::{FileMeta, RepMeta};
use std::sync::Mutex;

/// Order-independent hash of a rep set (and file set).
pub fn set_hash(reps: &[RepMeta], files: &[FileMeta]) -> [u8; 32] {
    let mut parts: Vec<Vec<u8>> = reps
        .iter()
        .map(|r| [r.mime.as_bytes(), &r.hash].concat())
        .chain(
            files
                .iter()
                .map(|f| [f.rel_path.as_bytes(), &f.hash].concat()),
        )
        .collect();
    parts.sort();
    let mut h = blake3::Hasher::new();
    for p in parts {
        h.update(&(p.len() as u64).to_le_bytes());
        h.update(&p);
    }
    *h.finalize().as_bytes()
}

#[derive(Default)]
pub struct EchoGuard(Mutex<Option<[u8; 32]>>);

impl EchoGuard {
    pub fn mark_applied(&self, h: [u8; 32]) {
        if let Ok(mut g) = self.0.lock() {
            *g = Some(h);
        }
    }

    pub fn is_echo(&self, h: &[u8; 32]) -> bool {
        self.0.lock().is_ok_and(|g| g.as_ref() == Some(h))
    }

    /// Something else was copied locally: the next identical copy is genuine.
    pub fn clear(&self) {
        if let Ok(mut g) = self.0.lock() {
            *g = None;
        }
    }
}
