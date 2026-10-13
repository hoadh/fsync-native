//! Sender-side registry of announced items so lazy pastes and resumes can be served.
//! Entry lifetime: `ttl`, or `grace` after a newer clip was published, whichever is first.

use super::served::{Served, Source};
use crate::blobs::Hash;
use fsync_proto::{ClipOffer, NodeId};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub struct Entry {
    pub offer: Arc<ClipOffer>,
    pub reps: Vec<Arc<Served>>,
    /// Indexed by `FileMeta::file_id` (ids are assigned as indices).
    pub files: Vec<Arc<Served>>,
    pub offered_to: HashSet<NodeId>,
    pub created: Instant,
    pub superseded: Option<Instant>,
}

pub struct Outbox {
    entries: HashMap<Uuid, Entry>,
    ttl: Duration,
    grace: Duration,
}

impl Outbox {
    pub fn new(ttl: Duration, grace: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            ttl,
            grace,
        }
    }

    fn expired(&self, e: &Entry, now: Instant) -> bool {
        now.duration_since(e.created) >= self.ttl
            || e.superseded
                .is_some_and(|s| now.duration_since(s) >= self.grace)
    }

    /// Drop expired entries; returns spilled blobs no remaining entry references.
    pub fn sweep(&mut self, now: Instant) -> Vec<Hash> {
        let dead: Vec<Uuid> = self
            .entries
            .iter()
            .filter(|(_, e)| self.expired(e, now))
            .map(|(k, _)| *k)
            .collect();
        let mut spilled = Vec::new();
        for id in dead {
            if let Some(e) = self.entries.remove(&id) {
                spilled.extend(e.reps.iter().filter_map(|r| blob_of(r)));
            }
        }
        spilled.retain(|h| {
            !self
                .entries
                .values()
                .any(|e| e.reps.iter().any(|r| blob_of(r) == Some(*h)))
        });
        spilled
    }

    /// Add a new item; every older item starts its grace period.
    pub fn insert(&mut self, entry: Entry, now: Instant) -> Vec<Hash> {
        for e in self.entries.values_mut() {
            e.superseded.get_or_insert(now);
        }
        self.entries.insert(entry.offer.item_id, entry);
        self.sweep(now)
    }

    pub fn get(&self, id: &Uuid, now: Instant) -> Option<&Entry> {
        self.entries.get(id).filter(|e| !self.expired(e, now))
    }

    /// Spilled blobs any live entry still serves.
    pub fn blob_hashes(&self) -> HashSet<Hash> {
        self.entries
            .values()
            .flat_map(|e| e.reps.iter().filter_map(|r| blob_of(r)))
            .collect()
    }

    /// Delivery failed: the peer never saw the offer, so it may not fetch.
    pub fn unoffer(&mut self, id: &Uuid, peer: &NodeId) {
        if let Some(e) = self.entries.get_mut(id) {
            e.offered_to.remove(peer);
        }
    }
}

fn blob_of(s: &Served) -> Option<Hash> {
    match s.source {
        Source::Blob(h) => Some(h),
        _ => None,
    }
}
