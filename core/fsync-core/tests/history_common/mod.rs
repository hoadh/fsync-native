#![allow(dead_code)]
pub mod spy;
use fsync_core::{
    EngineEvent, EventSink,
    blobs::{BlobStore, EncryptedBlobStore, FsBlobStore},
    history::{Direction, FileRow, FileState, History, NewItem, RepRow},
};
use fsync_proto::ClipKind;
use std::{path::Path, sync::Arc};
use tokio::sync::mpsc;
use uuid::Uuid;

pub const KEY: [u8; 32] = [7; 32];
pub const DAY_MS: u64 = 86_400_000;

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

pub struct Env {
    pub hist: History,
    pub store: Arc<dyn BlobStore>,
    pub events: mpsc::Receiver<EngineEvent>,
    pub dir: std::path::PathBuf,
}

pub fn blob_store(dir: &Path, key: &[u8; 32]) -> Arc<dyn BlobStore> {
    let fs = Arc::new(FsBlobStore::open(dir.join("blobs")).unwrap());
    Arc::new(EncryptedBlobStore::new(fs, key).unwrap())
}

pub fn open(dir: &Path, key: &[u8; 32]) -> Env {
    let (sink, events) = EventSink::channel();
    let store = blob_store(dir, key);
    let hist = History::open(&dir.join("data"), key, store.clone(), &sink).unwrap();
    Env {
        hist,
        store,
        events,
        dir: dir.to_owned(),
    }
}

pub fn text_item(preview: &str, created_ms: u64) -> NewItem {
    NewItem {
        id: Uuid::now_v7(),
        direction: Direction::In,
        peer: None,
        kind: ClipKind::Text,
        created_ms,
        preview: Some(preview.into()),
        sensitive: false,
        reps: vec![],
        files: vec![],
    }
}

/// Item with one rep whose bytes are stored in `store`.
pub fn blob_item(store: &dyn BlobStore, bytes: &[u8], created_ms: u64) -> NewItem {
    let hash = *blake3::hash(bytes).as_bytes();
    store.put(&hash, bytes).unwrap();
    let mut it = text_item("blob item", created_ms);
    it.reps.push(RepRow {
        idx: 0,
        mime: "text/plain".into(),
        size: bytes.len() as u64,
        hash,
        present: true,
    });
    it
}

pub fn file_row(hash: [u8; 32]) -> FileRow {
    FileRow {
        file_id: 0,
        name: "a.bin".into(),
        rel_path: "a.bin".into(),
        size: 10,
        hash,
        local_path: None,
        state: FileState::Offered,
    }
}

pub fn pseudo(len: usize, seed: u64) -> Vec<u8> {
    let mut s = seed | 1;
    (0..len)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 24) as u8
        })
        .collect()
}
