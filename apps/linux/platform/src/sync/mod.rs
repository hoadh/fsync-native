//! Sync engine between a clipboard backend and fsync-core: outbound (change -> filters ->
//! `publish_clip`), inbound (`on_remote_clip` -> eager fetch -> backend write or pending), one
//! lane each (newest wins). No Qt: the app supplies a `ClipService` and a status callback.

mod engine;
mod engine_types;
mod inbound;
mod lanes;
mod outbound;
mod shared;
mod shared_files;
mod shared_inbound;
mod status;
#[cfg(test)]
mod tests_fakes;
#[cfg(test)]
mod tests_files;
#[cfg(test)]
mod tests_inbound;
#[cfg(test)]
mod tests_modes;
#[cfg(test)]
mod tests_outbound;
#[cfg(test)]
mod tests_repeat;
#[cfg(test)]
mod tests_rig;

pub use engine::SyncEngine;
pub use engine_types::{FilesEnv, LargeHook, Paste, SyncEnv};
pub use inbound::ServiceFetcher;
pub use status::{PendingInfo, StatusCell, SyncStatus};

use fsync_core::service::{LocalClip, Published};

/// The slice of core's service the sync engine needs (the app implements it over
/// `FsyncService`; tests use a fake). Errors are user-facing sentences without content.
pub trait ClipService: Send + Sync {
    fn publish(&self, clip: LocalClip) -> Result<Published, String>;
    fn fetch_rep(&self, item_id: &str, idx: u32) -> Result<Vec<u8>, String>;
    fn settings(&self) -> SyncSettings;
    /// Downloads one file of a received item into `dest_dir` (blocking; core places it at
    /// `dest_dir/<rel_path>` and the final path is returned). `cancel` turns true when the
    /// result is no longer wanted. Without file support the default refuses.
    fn fetch_file(
        &self,
        _item_id: &str,
        _file_id: u32,
        _dest_dir: &std::path::Path,
        _cancel: &dyn Fn() -> bool,
        _deadline: std::time::Instant,
    ) -> Result<std::path::PathBuf, String> {
        Err("Receiving files is not supported here.".into())
    }
}

/// Core settings the clipboard cares about, read fresh for every change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncSettings {
    /// Signed in and online: otherwise changes are not read and received clips are dropped.
    pub active: bool,
    /// Core `clip_mode == Auto`.
    pub auto: bool,
    pub max_clip_bytes: u64,
    pub denylist: Vec<String>,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            active: true,
            auto: true,
            max_clip_bytes: 50 << 20,
            denylist: Vec::new(),
        }
    }
}
