//! Opening the SQLCipher database: raw key, WAL, migrations, key-loss recovery.

use super::{migrations::migrate, recovery};
use crate::{
    blobs::{BlobStore, Hash, blocking},
    error::{CoreError, Result},
    events::{EngineEvent, EventSink},
    fs_util::ensure_private_dir,
};
use rusqlite::{Connection, ErrorCode, OpenFlags};
use std::{
    collections::HashSet,
    fmt::Write,
    path::Path,
    sync::{Arc, Mutex},
};
use zeroize::Zeroizing;

pub const DB_FILE: &str = "history.db";

/// Hashes the engine still needs on disk (blobs of live outbox entries).
pub type BlobGuard = Arc<dyn Fn() -> HashSet<Hash> + Send + Sync>;

pub(crate) struct Inner {
    /// Single writer; WAL lets `reader` run concurrently.
    pub(crate) w: Arc<Mutex<Connection>>,
    pub(crate) r: Arc<Mutex<Connection>>,
    pub(crate) store: Arc<dyn BlobStore>,
    pub(crate) guard: Mutex<Option<BlobGuard>>,
}

/// Encrypted history database. Cheap to clone; every call runs on the blocking pool.
#[derive(Clone)]
pub struct History(pub(crate) Arc<Inner>);

fn keyed(path: &Path, key: &[u8; 32], flags: OpenFlags) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, flags)?;
    let mut hex = Zeroizing::new(String::with_capacity(70));
    hex.push_str("PRAGMA key = \"x'");
    for b in key {
        let _ = write!(hex, "{b:02x}");
    }
    hex.push_str("'\";");
    conn.execute_batch(&hex)?;
    // Wrong key / not a database surfaces on the first real read.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    })?;
    Ok(conn)
}

fn open_writer(path: &Path, key: &[u8; 32]) -> Result<Connection> {
    let rw = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE;
    let mut conn = keyed(path, key, rw | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    conn.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))?;
    conn.execute_batch(
        "PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON; PRAGMA secure_delete = ON;
         PRAGMA busy_timeout = 5000;",
    )?;
    migrate(&mut conn)?;
    Ok(conn)
}

fn open_reader(path: &Path, key: &[u8; 32]) -> Result<Connection> {
    let conn = keyed(
        path,
        key,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.execute_batch("PRAGMA busy_timeout = 5000; PRAGMA query_only = ON;")?;
    Ok(conn)
}

/// Wrong key or damaged file (as opposed to I/O or a too-new schema).
fn unreadable(e: &CoreError) -> bool {
    matches!(e, CoreError::Db(rusqlite::Error::SqliteFailure(f, _))
        if matches!(f.code, ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt))
}

impl History {
    /// Open (or create) `<dir>/history.db` keyed with `key`. A file that is corrupt under the key
    /// it was written with is moved aside, a fresh one is created and `HistoryReset` is emitted.
    /// A different key than the one on record fails with `HistoryKeyMismatch` and touches nothing;
    /// other failures (I/O, newer schema) are returned untouched. Call during startup.
    pub fn open(
        dir: &Path,
        key: &[u8; 32],
        store: Arc<dyn BlobStore>,
        events: &EventSink,
    ) -> Result<Self> {
        ensure_private_dir(dir)?;
        let path = dir.join(DB_FILE);
        let w = match open_writer(&path, key) {
            Err(e) if unreadable(&e) => {
                // Unreadable with the wrong key looks exactly like corruption. Only a file that
                // fails with the key it was written with is moved aside; a key that merely
                // differs (a flaky keychain read) leaves everything untouched.
                if recovery::key_verdict(dir, key) == recovery::KeyVerdict::Mismatch {
                    return Err(CoreError::HistoryKeyMismatch);
                }
                let backup = recovery::move_aside(dir)?;
                tracing::warn!("history database unreadable, starting fresh");
                events.emit(EngineEvent::HistoryReset { backup });
                let w = open_writer(&path, key)?;
                // Only here is the database known to be empty, the baseline for pruning.
                recovery::prune_backups(dir, &path);
                w
            }
            r => r?,
        };
        recovery::record_key(dir, key);
        let r = open_reader(&path, key)?;
        Ok(Self(Arc::new(Inner {
            w: Arc::new(Mutex::new(w)),
            r: Arc::new(Mutex::new(r)),
            store,
            guard: Mutex::new(None),
        })))
    }

    /// Blobs that must survive GC although no history row references them.
    pub fn set_blob_guard(&self, guard: BlobGuard) {
        if let Ok(mut g) = self.0.guard.lock() {
            *g = Some(guard);
        }
    }

    pub(crate) async fn write<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        run(self.0.w.clone(), f).await
    }

    pub(crate) async fn read<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        run(self.0.r.clone(), f).await
    }
}

async fn run<T: Send + 'static>(
    conn: Arc<Mutex<Connection>>,
    f: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
) -> Result<T> {
    blocking(move || {
        let mut g = conn
            .lock()
            .map_err(|_| CoreError::Store("history lock poisoned".into()))?;
        f(&mut g)
    })
    .await
}
