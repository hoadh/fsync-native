//! Helpers for file/1 tests.
#![allow(dead_code)]

use super::engine::{EngPeer, pseudo_random};
use fsync_core::{
    EngineEvent,
    clip::{ClipInput, FileInput, PublishOpts, PublishOutcome},
};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub fn write_src(dir: &Path, name: &str, len: usize, seed: u64) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, pseudo_random(len, seed)).unwrap();
    p
}

/// A offers `path` to everyone; returns what B saw.
pub async fn offer_file(ps: &mut [EngPeer], path: &Path) -> (Uuid, fsync_proto::FileMeta) {
    let (a, rest) = ps.split_first_mut().unwrap();
    let b = &mut rest[0];
    let out = a
        .engine
        .publish_clip(
            ClipInput::files(vec![FileInput::new(path)]),
            PublishOpts::default(),
        )
        .await
        .unwrap();
    assert!(matches!(out, PublishOutcome::Sent(_)));
    b.wait(|e| match e {
        EngineEvent::RemoteClip { offer, .. } => Some((offer.item_id, offer.files[0].clone())),
        _ => None,
    })
    .await
}

pub fn hash_file(p: &Path) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update_reader(std::fs::File::open(p).unwrap()).unwrap();
    *h.finalize().as_bytes()
}

pub fn part_len(b: &EngPeer, hash: &[u8; 32]) -> u64 {
    let name = format!("{}.part", blake3::Hash::from_bytes(*hash).to_hex());
    std::fs::metadata(b.blobs_dir().join(name)).map_or(0, |m| m.len())
}
