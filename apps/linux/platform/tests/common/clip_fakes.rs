//! Fakes for the clipboard tests: a core service that records publishes, a backend that remembers
//! writes, a reader over fixed bytes, and a temporary home with protected files. Real path rules
//! and real engine code run on top of these; only the display server and core are replaced.
#![allow(dead_code)]

use fsync_core::service::{LocalClip, Published};
use fsync_linux_platform::clipboard::path_denylist::PathDenylist;
use fsync_linux_platform::clipboard::{
    Backend, BackendKind, ChangeReader, ClipChange, OwnedEntry, ReadError, WriteError,
};
use fsync_linux_platform::sync::{ClipService, SyncSettings};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct Home {
    pub _dir: tempfile::TempDir,
    pub home: PathBuf,
}

pub fn home_with_secrets() -> Home {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    for d in [".ssh", ".sshkeys", "Documents", "data"] {
        std::fs::create_dir_all(home.join(d)).expect("mkdir");
    }
    std::fs::write(home.join(".ssh/id_ed25519"), b"PRIVATE").expect("key");
    std::fs::write(home.join("data/secret.db"), b"DB").expect("db");
    std::fs::write(home.join("Documents/a.txt"), b"hello").expect("doc");
    std::fs::write(home.join(".sshkeys/notes"), b"not a key dir").expect("sibling");
    Home { _dir: dir, home }
}

pub fn denylist_for(h: &Home) -> PathDenylist {
    PathDenylist::new(
        &h.home,
        &h.home.join("data"),
        &h.home.join(".local/share"),
        &h.home.join(".config"),
    )
}

pub type RepMap = Mutex<HashMap<String, Vec<(String, Vec<u8>)>>>;

pub struct Svc {
    pub published: Sender<LocalClip>,
    pub reps: RepMap,
    pub settings: Mutex<SyncSettings>,
}

impl ClipService for Svc {
    fn publish(&self, clip: LocalClip) -> Result<Published, String> {
        self.published.send(clip).map_err(|e| e.to_string())?;
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
        self.reps
            .lock()
            .unwrap()
            .get(item_id)
            .and_then(|v| v.get(idx as usize))
            .map(|(_, d)| d.clone())
            .ok_or_else(|| "gone".to_owned())
    }
    fn settings(&self) -> SyncSettings {
        self.settings.lock().unwrap().clone()
    }
}

/// Backend that only remembers what was written (the real backends are tested below).
pub struct MemBackend {
    pub rx: Mutex<Option<Receiver<ClipChange>>>,
    pub written: Mutex<Vec<OwnedEntry>>,
}

impl Backend for MemBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::X11
    }
    fn take_changes(&self) -> Option<Receiver<ClipChange>> {
        self.rx.lock().unwrap().take()
    }
    fn write(&self, entry: OwnedEntry) -> Result<(), WriteError> {
        self.written.lock().unwrap().push(entry);
        Ok(())
    }
    fn owned(&self) -> bool {
        true
    }
    fn shutdown(&self) {}
}

pub struct MemReader(pub HashMap<String, Vec<u8>>);

impl ChangeReader for MemReader {
    fn read(&self, mime: &str, cap: usize, _t: Duration) -> Result<Vec<u8>, ReadError> {
        let d = self.0.get(mime).ok_or(ReadError::Stale)?;
        if d.len() > cap {
            Err(ReadError::TooLarge)
        } else {
            Ok(d.clone())
        }
    }
}

pub fn change(parts: &[(&str, &[u8])]) -> ClipChange {
    ClipChange {
        mimes: parts.iter().map(|(m, _)| (*m).to_owned()).collect(),
        source: None,
        reader: Arc::new(MemReader(
            parts
                .iter()
                .map(|(m, d)| ((*m).to_owned(), d.to_vec()))
                .collect(),
        )),
    }
}
