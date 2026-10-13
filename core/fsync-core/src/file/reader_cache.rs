//! Bounded LRU of open `RangeReader`s, tied to the received-offer log.

use super::range_reader::RangeReader;
use std::{collections::VecDeque, sync::Arc};
use uuid::Uuid;

/// Open readers kept at once; the least recently used is shut down beyond this.
pub const MAX_READERS: usize = 8;

type Key = (Uuid, u32);

#[derive(Default)]
pub struct ReaderCache {
    /// Most recently used last.
    list: VecDeque<(Key, Arc<RangeReader>)>,
}

impl ReaderCache {
    pub fn get_or_insert_with(
        &mut self,
        key: Key,
        make: impl FnOnce() -> Arc<RangeReader>,
    ) -> Arc<RangeReader> {
        if let Some(i) = self.list.iter().position(|(k, _)| *k == key)
            && let Some(e) = self.list.remove(i)
        {
            let r = e.1.clone();
            self.list.push_back(e);
            return r;
        }
        let r = make();
        self.list.push_back((key, r.clone()));
        while self.list.len() > MAX_READERS {
            if let Some((_, old)) = self.list.pop_front() {
                old.shutdown();
            }
        }
        r
    }

    /// The offer was forgotten: drop its readers.
    pub fn evict_items(&mut self, items: &[Uuid]) {
        self.list.retain(|((item, _), r)| {
            let keep = !items.contains(item);
            if !keep {
                r.shutdown();
            }
            keep
        });
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}
