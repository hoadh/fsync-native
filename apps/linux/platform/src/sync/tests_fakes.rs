//! Fake backend and fake core service for the engine tests.

use super::tests_rig::RepList;
use super::{ClipService, SyncSettings};
use crate::clipboard::{Backend, BackendKind, ClipChange, OwnedEntry, WriteError};
use fsync_core::service::{LocalClip, Published};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

pub(super) struct FakeBackend {
    pub(super) rx: Mutex<Option<Receiver<ClipChange>>>,
    pub(super) written: Mutex<Vec<OwnedEntry>>,
    pub(super) shut: AtomicUsize,
    pub(super) fail_big: bool,
}

impl Backend for FakeBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::X11
    }
    fn take_changes(&self) -> Option<Receiver<ClipChange>> {
        self.rx.lock().expect("rx").take()
    }
    fn write(&self, entry: OwnedEntry) -> Result<(), WriteError> {
        if self.fail_big && entry.total_bytes() > 1000 {
            return Err(WriteError::TooLargeForX11(entry.total_bytes()));
        }
        self.written.lock().expect("w").push(entry);
        Ok(())
    }
    fn owned(&self) -> bool {
        true
    }
    fn shutdown(&self) {
        self.shut.fetch_add(1, Ordering::SeqCst);
    }
}

pub(super) struct FakeService {
    pub(super) published: Sender<LocalClip>,
    pub(super) reps: Mutex<HashMap<String, RepList>>,
    pub(super) settings: Mutex<SyncSettings>,
    pub(super) fetch_delay: Mutex<Duration>,
    pub(super) fetched: AtomicUsize,
    pub(super) publish_delay: Mutex<Duration>,
    /// Relative paths of the files of an item, written by `fetch_file`.
    pub(super) file_rels: Mutex<HashMap<String, Vec<String>>>,
    pub(super) file_fetches: AtomicUsize,
}

impl ClipService for FakeService {
    fn publish(&self, clip: LocalClip) -> Result<Published, String> {
        std::thread::sleep(*self.publish_delay.lock().expect("p"));
        self.published.send(clip).expect("record");
        Ok(Published {
            item_id: "i".into(),
            echo: false,
            delivered: vec!["n".into()],
            failed: vec![],
            pending: vec![],
            refused: vec![],
        })
    }
    fn fetch_rep(&self, item_id: &str, idx: u32) -> Result<Vec<u8>, String> {
        self.fetched.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(*self.fetch_delay.lock().expect("d"));
        self.reps
            .lock()
            .expect("r")
            .get(item_id)
            .and_then(|v| v.get(idx as usize))
            .map(|(_, d)| d.clone())
            .ok_or("gone".into())
    }
    fn settings(&self) -> SyncSettings {
        self.settings.lock().expect("s").clone()
    }
    fn fetch_file(
        &self,
        item_id: &str,
        file_id: u32,
        dest: &std::path::Path,
        _cancel: &dyn Fn() -> bool,
        _deadline: std::time::Instant,
    ) -> Result<std::path::PathBuf, String> {
        self.file_fetches.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(*self.fetch_delay.lock().expect("d"));
        let rel = self
            .file_rels
            .lock()
            .expect("f")
            .get(item_id)
            .and_then(|v| v.get(file_id as usize).cloned())
            .ok_or("gone")?;
        let p = dest.join(rel);
        std::fs::create_dir_all(p.parent().expect("parent")).map_err(|e| e.to_string())?;
        std::fs::write(&p, b"file-bytes").map_err(|e| e.to_string())?;
        Ok(p)
    }
}
