//! Bounded LRU of open `RangeReader`s, tied to the received-offer log.

use super::range_reader::RangeReader;
use fsync_proto::NodeId;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use tokio::sync::Semaphore;
use uuid::Uuid;

/// Open readers kept at once; the least recently used is shut down beyond this.
pub const MAX_READERS: usize = 8;

/// Speculative window fetches in flight per peer, over all its readers. The sender serves 16
/// streams per connection, so half stay free for reads a caller is waiting on.
pub const PREFETCH_PER_PEER: usize = 8;

type Key = (Uuid, u32);

#[derive(Default)]
pub struct ReaderCache {
    /// Most recently used last.
    list: VecDeque<(Key, Arc<RangeReader>)>,
    budgets: HashMap<NodeId, Arc<Semaphore>>,
}

impl ReaderCache {
    /// The prefetch budget of `peer` (created on first use).
    pub fn budget(&mut self, peer: NodeId) -> Arc<Semaphore> {
        self.budgets
            .entry(peer)
            .or_insert_with(|| Arc::new(Semaphore::new(PREFETCH_PER_PEER)))
            .clone()
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use fsync_proto::NodeId;

    fn node(seed: u8) -> NodeId {
        NodeId::from(&ed25519_dalek::SigningKey::from_bytes(&[seed; 32]))
    }

    #[test]
    fn prefetch_budget_is_one_per_peer_and_bounded() {
        let mut c = ReaderCache::default();
        let (a1, a2, b) = (c.budget(node(1)), c.budget(node(1)), c.budget(node(2)));
        assert!(
            Arc::ptr_eq(&a1, &a2),
            "all readers of a peer share one budget"
        );
        assert!(!Arc::ptr_eq(&a1, &b));
        let held: Vec<_> = (0..PREFETCH_PER_PEER)
            .map(|_| a1.clone().try_acquire_owned().unwrap())
            .collect();
        assert!(
            a1.clone().try_acquire_owned().is_err(),
            "no permit beyond the budget"
        );
        assert!(
            b.clone().try_acquire_owned().is_ok(),
            "other peers are unaffected"
        );
        drop(held);
        assert!(a1.try_acquire_owned().is_ok());
    }
}
