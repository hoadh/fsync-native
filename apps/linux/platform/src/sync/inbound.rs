//! Inbound pipeline: a received `ClipItem` is fetched completely (every rep, eager) and only
//! then turned into one `OwnedEntry`; nothing lazy is ever put on the clipboard. Runs on the
//! inbound lane, never on core's callback thread.

use super::ClipService;
use super::lanes::Cancel;
use crate::clipboard::OwnedEntry;
use crate::clipboard::formats::to_entry;
use crate::files::inbox::FileFetcher;
use fsync_core::service::{ClipItem, ClipRep};
use fsync_proto::ClipKind;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Whole-clip budget, checked between reps (a rep in flight is bounded by core).
const FETCH_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, PartialEq, Eq)]
pub enum Fetched {
    Entry(OwnedEntry),
    /// A newer clip superseded this one.
    Cancelled,
    /// Nothing in it can be written (unknown formats, oversize image).
    Unwritable,
    Failed(String),
}

pub fn kind_name(kind: ClipKind) -> &'static str {
    match kind {
        ClipKind::Text => "text",
        ClipKind::Rich => "rich text",
        ClipKind::Image => "image",
        ClipKind::Files => "files",
    }
}

/// Fetches every rep of `item`; checks `cancel` between reps (a fetch in flight cannot be
/// interrupted, its result is simply dropped).
pub fn fetch_all(service: &dyn ClipService, item: &ClipItem, cancel: &Cancel) -> Fetched {
    let started = std::time::Instant::now();
    let mut reps = Vec::with_capacity(item.reps.len());
    for (idx, info) in item.reps.iter().enumerate() {
        if cancel.is_cancelled() {
            return Fetched::Cancelled;
        }
        if started.elapsed() > FETCH_BUDGET {
            return Fetched::Failed("it took too long to arrive".into());
        }
        let Ok(idx) = u32::try_from(idx) else {
            return Fetched::Unwritable;
        };
        match service.fetch_rep(&item.item_id, idx) {
            Ok(data) => reps.push(ClipRep {
                mime: info.mime.clone(),
                data,
            }),
            Err(e) => return Fetched::Failed(e),
        }
    }
    if cancel.is_cancelled() {
        return Fetched::Cancelled;
    }
    let entry = to_entry(&reps);
    if entry.reps.is_empty() {
        Fetched::Unwritable
    } else {
        Fetched::Entry(entry)
    }
}

/// A `ClipService` seen as the inbox's file fetcher.
pub struct ServiceFetcher<'a>(pub &'a dyn ClipService);

impl FileFetcher for ServiceFetcher<'_> {
    fn fetch_file(
        &self,
        item_id: &str,
        file_id: u32,
        dest_dir: &Path,
        cancel: &dyn Fn() -> bool,
        deadline: Instant,
    ) -> Result<PathBuf, String> {
        self.0
            .fetch_file(item_id, file_id, dest_dir, cancel, deadline)
    }
}
