//! Engine tests with a fake backend and a fake core service (no display, no network).

use super::*;
use crate::clipboard::path_denylist::PathDenylist;
use crate::clipboard::{Backend, ChangeReader, ClipChange, ReadError, SourceApp};
use crate::files::inbox::Inbox;
use fsync_core::service::{ClipItem, LocalClip, RepInfo};
use fsync_proto::ClipKind;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::tests_fakes::*;

/// `(mime, bytes)` per rep of an item.
pub(super) type RepList = Vec<(String, Vec<u8>)>;

pub(super) struct MemReader {
    pub(super) data: HashMap<String, Vec<u8>>,
    pub(super) reads: AtomicUsize,
}

impl ChangeReader for MemReader {
    fn read(&self, mime: &str, cap: usize, _t: Duration) -> Result<Vec<u8>, ReadError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let d = self.data.get(mime).ok_or(ReadError::Stale)?;
        if d.len() > cap {
            Err(ReadError::TooLarge)
        } else {
            Ok(d.clone())
        }
    }
}

pub(super) fn change(
    parts: &[(&str, &[u8])],
    source: Option<SourceApp>,
) -> (ClipChange, Arc<MemReader>) {
    let reader = Arc::new(MemReader {
        data: parts
            .iter()
            .map(|(m, d)| ((*m).to_owned(), d.to_vec()))
            .collect(),
        reads: AtomicUsize::new(0),
    });
    let c = ClipChange {
        mimes: parts.iter().map(|(m, _)| (*m).to_owned()).collect(),
        source,
        reader: reader.clone(),
    };
    (c, reader)
}

pub(super) struct Rig {
    pub(super) engine: Arc<SyncEngine>,
    pub(super) backend: Arc<FakeBackend>,
    pub(super) service: Arc<FakeService>,
    pub(super) tx: Sender<ClipChange>,
    pub(super) published: Receiver<LocalClip>,
    pub(super) _home: tempfile::TempDir,
    pub(super) home: std::path::PathBuf,
}

pub(super) fn rig(with_backend: bool, auto: bool, fail_big: bool) -> Rig {
    rig_with(with_backend, auto, fail_big, None)
}

pub(super) fn rig_with(
    with_backend: bool,
    auto: bool,
    fail_big: bool,
    on_large: Option<LargeHook>,
) -> Rig {
    let (tx, rx) = mpsc::channel();
    let (ptx, prx) = mpsc::channel();
    let home_dir = tempfile::tempdir().expect("tmp");
    let home = home_dir.path().to_path_buf();
    for d in [".ssh", "Documents", "data"] {
        std::fs::create_dir_all(home.join(d)).expect("mkdir");
    }
    let backend = Arc::new(FakeBackend {
        rx: Mutex::new(Some(rx)),
        written: Mutex::new(vec![]),
        shut: AtomicUsize::new(0),
        fail_big,
    });
    let service = Arc::new(FakeService {
        published: ptx,
        reps: Mutex::new(HashMap::new()),
        settings: Mutex::new(SyncSettings {
            active: true,
            auto,
            max_clip_bytes: 10_000,
            denylist: vec![],
        }),
        fetch_delay: Mutex::new(Duration::ZERO),
        fetched: AtomicUsize::new(0),
        publish_delay: Mutex::new(Duration::ZERO),
        file_rels: Mutex::new(HashMap::new()),
        file_fetches: AtomicUsize::new(0),
    });
    let env = SyncEnv {
        paths: PathDenylist::new(
            &home,
            &home.join("data"),
            &home.join(".local/share"),
            &home.join(".config"),
        ),
        files: on_large.map(|on_large| FilesEnv {
            inbox: Inbox::new(home.join("inbox")),
            on_large,
        }),
    };
    let b: Option<Arc<dyn Backend>> = with_backend.then(|| backend.clone() as Arc<dyn Backend>);
    let engine = SyncEngine::start(
        service.clone(),
        b,
        env,
        SyncStatus::default(),
        Arc::new(|| {}),
    )
    .expect("engine");
    Rig {
        engine,
        backend,
        service,
        tx,
        published: prx,
        _home: home_dir,
        home,
    }
}

pub(super) fn item(id: &str, kind: ClipKind, reps: &[(&str, &[u8])], rig: &Rig) -> ClipItem {
    rig.service.reps.lock().expect("r").insert(
        id.into(),
        reps.iter()
            .map(|(m, d)| ((*m).into(), d.to_vec()))
            .collect(),
    );
    ClipItem {
        item_id: id.into(),
        from: "node".into(),
        from_name: "phone".into(),
        kind,
        ts_ms: 0,
        preview: None,
        reps: reps
            .iter()
            .map(|(m, d)| RepInfo {
                mime: (*m).into(),
                size: d.len() as u64,
            })
            .collect(),
        files: vec![],
        total_size: reps.iter().map(|(_, d)| d.len() as u64).sum(),
        push: false,
    }
}

pub(super) fn wait<T>(mut f: impl FnMut() -> Option<T>) -> T {
    for _ in 0..200 {
        if let Some(v) = f() {
            return v;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timeout");
}

/// A file clip of `files` (relative path, size), registered with the fake service.
pub(super) fn file_item(id: &str, files: &[(&str, u64)], rig: &Rig) -> ClipItem {
    rig.service.file_rels.lock().expect("f").insert(
        id.into(),
        files.iter().map(|(p, _)| (*p).to_owned()).collect(),
    );
    let mut it = item(id, ClipKind::Files, &[], rig);
    it.files = files
        .iter()
        .enumerate()
        .map(|(i, (p, s))| fsync_core::service::FileInfo {
            file_id: i as u32,
            name: (*p).into(),
            rel_path: (*p).into(),
            size: *s,
        })
        .collect();
    it.total_size = files.iter().map(|(_, s)| s).sum();
    it
}
