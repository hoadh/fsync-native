//! Plain snapshots for the file features: pushed-file offers, the History rows and the large file
//! clips that wait for a download. Worker threads change them under the mutexes and poke the Qt
//! models, which copy the snapshot on the GUI thread (same pattern as the device list).

use fsync_core::service::ClipItem;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub use super::offers_state::{Admit, OfferRow, OfferState, admit_offer};

/// Desktop notifications for new offers are coalesced within this window.
const NOTIFY_EVERY: Duration = Duration::from_secs(5);
/// Most large file clips held for the History Download button.
pub const MAX_LARGE: usize = 32;

/// One line of the History view.
#[derive(Clone, Debug)]
pub struct HistoryRow {
    pub item_id: String,
    /// "text", "rich text", "image" or "files".
    pub kind: &'static str,
    pub preview: String,
    pub peer_name: String,
    pub created_ms: u64,
    pub total_size: u64,
    pub pinned: bool,
    pub outgoing: bool,
    pub can_copy: bool,
    pub can_download: bool,
    /// A download of this item is running (the button is disabled).
    pub downloading: bool,
}

#[derive(Default)]
pub struct HistoryUi {
    pub rows: Vec<HistoryRow>,
    pub query: String,
    /// Outcome of the last action ("Copied.", "Could not copy: ...").
    pub message: String,
    pub loading: bool,
}

#[derive(Default)]
pub struct FilesState {
    pub offers: Mutex<Vec<OfferRow>>,
    pub history: Mutex<HistoryUi>,
    /// Large file clips (newest last): the History Download button needs their file list.
    pub large: Mutex<Vec<ClipItem>>,
    /// Last time offer progress was pushed to the UI (progress is limited to 4 updates/s).
    pub last_progress_poke: Mutex<Option<Instant>>,
    last_notify: Mutex<Option<Instant>>,
    /// Bumped by sign-out and shutdown: downloads started earlier see it change and stop.
    epoch: AtomicU64,
}

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl FilesState {
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    /// Stops every running download and accepted offer (shutdown, sign-out).
    pub fn cancel_all(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        for o in lock(&self.offers).iter() {
            o.cancel.store(true, Ordering::SeqCst);
        }
    }

    /// Sign-out: cancels everything and forgets the account's offers, held clips and rows.
    pub fn reset(&self) {
        self.cancel_all();
        lock(&self.offers).clear();
        lock(&self.large).clear();
        let mut h = lock(&self.history);
        h.rows.clear();
        h.message.clear();
        h.query.clear();
        h.loading = false;
    }

    /// True at most once per `NOTIFY_EVERY` (the badge covers the rest).
    pub fn notify_due(&self) -> bool {
        let mut last = lock(&self.last_notify);
        if last.is_none_or(|t| t.elapsed() >= NOTIFY_EVERY) {
            *last = Some(Instant::now());
            true
        } else {
            false
        }
    }

    /// Holds `item` for a later download (replaces an older copy of the same item).
    pub fn hold_large(&self, item: ClipItem) {
        let mut v = lock(&self.large);
        v.retain(|i| i.item_id != item.item_id);
        v.push(item);
        if v.len() > MAX_LARGE {
            v.remove(0);
        }
    }

    pub fn large_item(&self, item_id: &str) -> Option<ClipItem> {
        lock(&self.large)
            .iter()
            .find(|i| i.item_id == item_id)
            .cloned()
    }

    pub fn is_large_held(&self, item_id: &str) -> bool {
        lock(&self.large).iter().any(|i| i.item_id == item_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fsync_proto::ClipKind;

    fn item(id: &str) -> ClipItem {
        ClipItem {
            item_id: id.into(),
            from: "n".into(),
            from_name: String::new(),
            kind: ClipKind::Files,
            ts_ms: 0,
            preview: None,
            reps: vec![],
            files: vec![],
            total_size: 10,
            push: true,
        }
    }

    #[test]
    fn large_clips_are_bounded_and_deduplicated() {
        let s = FilesState::default();
        for i in 0..(MAX_LARGE + 5) {
            s.hold_large(item(&format!("{i:x}")));
        }
        s.hold_large(item("20"));
        assert_eq!(lock(&s.large).len(), MAX_LARGE);
        assert!(!s.is_large_held("0"), "oldest dropped");
        assert!(s.large_item("20").is_some());
    }

    #[test]
    fn progress_never_exceeds_the_total_and_a_nameless_sender_is_named() {
        let mut o = OfferRow::new(&item("a"));
        assert_eq!(o.from_name, "Another device");
        o.finished_bytes = 8;
        o.active_bytes = 8;
        assert_eq!(o.done_bytes(), 10);
    }

    #[test]
    fn reset_cancels_running_work_and_forgets_the_account() {
        let s = FilesState::default();
        let row = OfferRow::new(&item("a"));
        let flag = row.cancel.clone();
        lock(&s.offers).push(row);
        s.hold_large(item("b"));
        lock(&s.history).message = "x".into();
        let e = s.epoch();
        s.reset();
        assert!(flag.load(Ordering::SeqCst));
        assert_ne!(s.epoch(), e);
        assert!(lock(&s.offers).is_empty() && lock(&s.large).is_empty());
        assert!(lock(&s.history).message.is_empty());
    }

    #[test]
    fn notifications_are_coalesced() {
        let s = FilesState::default();
        assert!(s.notify_due());
        assert!(!s.notify_due());
    }
}
