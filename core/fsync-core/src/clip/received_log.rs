//! What a receiver remembers of recent offers (dedupe ids + the offers needed for later fetches).

use crate::error::{CoreError, Result};
use fsync_proto::{ClipOffer, NodeId, RepMeta};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};
use uuid::Uuid;

const SEEN_CAP: usize = 1024;
const KEEP_CAP: usize = 128;
/// Memory budget for kept offers (inline bytes, names, previews).
pub const KEEP_BYTES: usize = 32 << 20;
/// Rough per-file bookkeeping beyond its name.
const FILE_OVERHEAD: usize = 100;

pub struct ReceivedOffer {
    pub from: NodeId,
    pub offer: Arc<ClipOffer>,
}

impl ReceivedOffer {
    /// Approximate heap held by this entry.
    fn weight(&self) -> usize {
        let o = &self.offer;
        let inline: usize = o.inline.iter().flatten().map(Vec::len).sum();
        let files: usize = o
            .files
            .iter()
            .map(|f| f.rel_path.len() + FILE_OVERHEAD)
            .sum();
        inline + files + o.preview.as_ref().map_or(0, String::len) + 256
    }
}

/// Sum of rep sizes; an overflowing (lying) total is rejected rather than wrapped.
pub fn checked_total(reps: &[RepMeta]) -> Result<u64> {
    reps.iter()
        .try_fold(0u64, |a, r| a.checked_add(r.size))
        .ok_or(CoreError::Invalid("rep sizes overflow"))
}

/// Dedupe ring (ids only) plus the most recent offers, bounded by count and bytes.
#[derive(Default)]
pub struct ReceivedLog {
    seen: HashSet<Uuid>,
    seen_order: VecDeque<Uuid>,
    kept: HashMap<Uuid, Arc<ReceivedOffer>>,
    kept_order: VecDeque<Uuid>,
    bytes: usize,
}

impl ReceivedLog {
    /// False if `id` was already seen.
    pub fn first_time(&mut self, id: Uuid) -> bool {
        if !self.seen.insert(id) {
            return false;
        }
        self.seen_order.push_back(id);
        if self.seen_order.len() > SEEN_CAP
            && let Some(old) = self.seen_order.pop_front()
        {
            self.seen.remove(&old);
        }
        true
    }

    /// Remember `r`; returns the ids evicted to stay within the count and byte budgets.
    pub fn keep(&mut self, r: Arc<ReceivedOffer>) -> Vec<Uuid> {
        let id = r.offer.item_id;
        self.bytes += r.weight();
        self.kept.insert(id, r);
        self.kept_order.push_back(id);
        let mut gone = Vec::new();
        // The newest entry always stays, even if it alone exceeds the budget.
        while self.kept_order.len() > 1
            && (self.kept_order.len() > KEEP_CAP || self.bytes > KEEP_BYTES)
        {
            let Some(old) = self.kept_order.pop_front() else {
                break;
            };
            if let Some(r) = self.kept.remove(&old) {
                self.bytes = self.bytes.saturating_sub(r.weight());
            }
            gone.push(old);
        }
        gone
    }

    pub fn get(&self, id: &Uuid) -> Option<Arc<ReceivedOffer>> {
        self.kept.get(id).cloned()
    }

    pub fn kept_bytes(&self) -> usize {
        self.bytes
    }
}
