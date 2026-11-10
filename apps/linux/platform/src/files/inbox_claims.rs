//! Download claims: one download per item at a time, shared by every clone of the `Inbox`.

use super::inbox::{FetchError, Inbox, failed};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// Marks an item as being downloaded; released on drop.
#[derive(Debug)]
pub struct DownloadGuard {
    pub(super) set: Arc<Mutex<HashSet<String>>>,
    pub(super) id: String,
}

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        self.set
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&self.id);
    }
}

impl Inbox {
    pub(super) fn flights(&self) -> std::sync::MutexGuard<'_, HashSet<String>> {
        self.in_flight.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Claims `item_id` for a download; a second claim while one runs is refused, because two
    /// downloads would clear each other's directory.
    pub fn begin_download(&self, item_id: &str) -> Result<DownloadGuard, FetchError> {
        self.item_dir(item_id)?;
        if !self.flights().insert(item_id.to_owned()) {
            return failed("Already downloading.");
        }
        Ok(DownloadGuard {
            set: Arc::clone(&self.in_flight),
            id: item_id.to_owned(),
        })
    }

    pub fn is_downloading(&self, item_id: &str) -> bool {
        self.flights().contains(item_id)
    }

    /// Items being downloaded right now (retention must not touch them).
    pub fn downloading(&self) -> Vec<String> {
        self.flights().iter().cloned().collect()
    }
}
