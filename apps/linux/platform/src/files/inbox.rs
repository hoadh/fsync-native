//! The eager inbox: `<root>/<item_id>/` holds one received file clip, downloaded completely.
//! A `.complete` marker (the top-level entry names, written last) tells a finished copy from a
//! partial one; a finished copy is reused instead of fetched again. Every name that came from
//! the network is validated here once more (core validates it too): the inbox never trusts a
//! remote path component.

pub use super::inbox_claims::DownloadGuard;
use super::names::valid_rel_path;
pub use super::names::{valid_component, valid_item_id};
use crate::paths::create_private_dir;
use fsync_core::service::ClipItem;
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Whole-item budget (ADR 013 d6).
pub const FETCH_BUDGET: Duration = Duration::from_secs(180);
pub(super) const COMPLETE: &str = ".complete";
/// Written after a download the user asked for: retention then only applies the age limit.
pub const EXPLICIT: &str = ".explicit";

/// Downloads one file of a received item into `dest_dir` (core places it at `dest_dir/rel_path`
/// and returns the final path). `cancel` turns true when the result is no longer wanted.
pub trait FileFetcher {
    fn fetch_file(
        &self,
        item_id: &str,
        file_id: u32,
        dest_dir: &Path,
        cancel: &dyn Fn() -> bool,
        deadline: Instant,
    ) -> Result<PathBuf, String>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum FetchError {
    /// A newer clip superseded this one.
    Cancelled,
    /// A sentence for the user; never contains file names.
    Failed(String),
}

pub(super) fn failed<T>(text: &str) -> Result<T, FetchError> {
    Err(FetchError::Failed(text.to_owned()))
}

/// Clones share the in-flight set, so one `Inbox` per app serves every download path.
#[derive(Clone, Debug)]
pub struct Inbox {
    root: PathBuf,
    pub(super) in_flight: Arc<Mutex<HashSet<String>>>,
}

impl Inbox {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            in_flight: Arc::default(),
        }
    }

    /// Records that the user asked for this item: retention keeps it until it ages out.
    pub fn mark_explicit(&self, item_id: &str) {
        let Ok(dir) = self.item_dir(item_id) else {
            return;
        };
        if let Err(e) = std::fs::write(dir.join(EXPLICIT), b"") {
            tracing::warn!("cannot mark an explicit download: {e}");
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `<root>/<item_id>` for a valid id.
    pub fn item_dir(&self, item_id: &str) -> Result<PathBuf, FetchError> {
        if !valid_item_id(item_id) {
            return failed("The clip has an invalid id.");
        }
        Ok(self.root.join(item_id))
    }

    /// The top-level entries of a finished copy (absolute), `None` when there is none or it is
    /// damaged (a listed entry is missing).
    pub fn complete_entries(&self, item_id: &str) -> Option<Vec<PathBuf>> {
        let dir = self.item_dir(item_id).ok()?;
        let meta = std::fs::symlink_metadata(&dir).ok()?;
        if !meta.is_dir() {
            return None;
        }
        let dir = std::fs::canonicalize(&dir).ok()?;
        let text = std::fs::read_to_string(dir.join(COMPLETE)).ok()?;
        let mut out = Vec::new();
        for name in text.lines() {
            if !valid_component(name) {
                return None;
            }
            let p = dir.join(name);
            std::fs::symlink_metadata(&p).ok()?;
            out.push(p);
        }
        (!out.is_empty()).then_some(out)
    }

    /// Downloads every file of `item` (or reuses a finished copy) and returns the top-level
    /// entries to put on the clipboard (a folder is one entry).
    pub fn fetch_item(
        &self,
        fetcher: &dyn FileFetcher,
        item: &ClipItem,
        cancel: &dyn Fn() -> bool,
    ) -> Result<Vec<PathBuf>, FetchError> {
        self.fetch_item_within(fetcher, item, cancel, FETCH_BUDGET)
    }

    /// `fetch_item` with an explicit time budget (a download the user asked for may be big).
    pub fn fetch_item_within(
        &self,
        fetcher: &dyn FileFetcher,
        item: &ClipItem,
        cancel: &dyn Fn() -> bool,
        budget: Duration,
    ) -> Result<Vec<PathBuf>, FetchError> {
        let guard = self.begin_download(&item.item_id)?;
        self.fetch_item_guarded(&guard, fetcher, item, cancel, budget)
    }

    /// Like `fetch_item_within` for a caller that already holds the claim (it showed a busy row).
    pub fn fetch_item_guarded(
        &self,
        guard: &DownloadGuard,
        fetcher: &dyn FileFetcher,
        item: &ClipItem,
        cancel: &dyn Fn() -> bool,
        budget: Duration,
    ) -> Result<Vec<PathBuf>, FetchError> {
        if guard.id != item.item_id {
            return failed("The clip has an invalid id.");
        }
        let dir = self.item_dir(&item.item_id)?;
        if item.files.is_empty() {
            return failed("The clip has no files.");
        }
        if item.files.iter().any(|f| !valid_rel_path(&f.rel_path)) {
            return failed("The clip has an unsafe file name.");
        }
        if let Some(done) = self.complete_entries(&item.item_id) {
            touch(&dir);
            return Ok(done);
        }
        let canonical = self.fresh_dir(&dir)?;
        let result = self.download(fetcher, item, &canonical, cancel, budget);
        if result.is_err() {
            // A partial copy is useless (core keeps its own resume state); free the space now.
            let _ = std::fs::remove_dir_all(&canonical);
        }
        result
    }

    /// An empty private directory for the item; a partial earlier attempt is cleared. A link or
    /// plain file under the id is never followed or used.
    fn fresh_dir(&self, dir: &Path) -> Result<PathBuf, FetchError> {
        create_private_dir(&self.root).map_err(|e| {
            tracing::warn!("cannot create the inbox: {e}");
            FetchError::Failed("The inbox folder cannot be created.".into())
        })?;
        match std::fs::symlink_metadata(dir) {
            Ok(m) if m.is_dir() => {
                std::fs::remove_dir_all(dir).map_err(|e| io_failure("clear a partial copy", &e))?
            }
            Ok(_) => {
                std::fs::remove_file(dir).map_err(|e| io_failure("replace a stray file", &e))?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_failure("inspect the inbox", &e)),
        }
        create_private_dir(dir).map_err(|e| {
            tracing::warn!("cannot create the item folder: {e}");
            FetchError::Failed("The inbox folder cannot be created.".into())
        })?;
        std::fs::canonicalize(dir).map_err(|e| io_failure("resolve the inbox", &e))
    }

    fn download(
        &self,
        fetcher: &dyn FileFetcher,
        item: &ClipItem,
        dir: &Path,
        cancel: &dyn Fn() -> bool,
        budget: Duration,
    ) -> Result<Vec<PathBuf>, FetchError> {
        let deadline = Instant::now() + budget;
        let mut tops: Vec<String> = Vec::new();
        for f in &item.files {
            if cancel() {
                return Err(FetchError::Cancelled);
            }
            if Instant::now() >= deadline {
                return failed("It took too long to arrive.");
            }
            let placed = fetcher
                .fetch_file(&item.item_id, f.file_id, dir, cancel, deadline)
                .map_err(|e| {
                    if cancel() {
                        FetchError::Cancelled
                    } else {
                        FetchError::Failed(e)
                    }
                })?;
            let top = placed
                .strip_prefix(dir)
                .ok()
                .and_then(|rel| rel.components().next())
                .and_then(|c| c.as_os_str().to_str())
                .filter(|n| valid_component(n))
                .map(str::to_owned);
            let Some(top) = top else {
                return failed("A file was saved outside the inbox and was dropped.");
            };
            if !tops.contains(&top) {
                tops.push(top);
            }
        }
        if cancel() {
            return Err(FetchError::Cancelled);
        }
        write_marker(dir, &tops).map_err(|e| io_failure("finish the copy", &e))?;
        Ok(tops.iter().map(|n| dir.join(n)).collect())
    }
}

fn io_failure(what: &str, e: &std::io::Error) -> FetchError {
    tracing::warn!("inbox: cannot {what}: {e}");
    FetchError::Failed("The files could not be saved.".into())
}

/// Atomic: a half-written marker must never mark a copy complete.
fn write_marker(dir: &Path, names: &[String]) -> std::io::Result<()> {
    let tmp = dir.join(".complete.tmp");
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(names.join("\n").as_bytes())?;
    f.sync_all()?;
    std::fs::rename(&tmp, dir.join(COMPLETE))
}

/// Marks the copy as freshly used so retention keeps it (best effort).
fn touch(dir: &Path) {
    if let Ok(f) = std::fs::File::open(dir) {
        let _ = f.set_modified(std::time::SystemTime::now());
    }
}

#[cfg(test)]
#[path = "inbox_tests.rs"]
mod tests;
