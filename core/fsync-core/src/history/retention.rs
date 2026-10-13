//! Deleting items and garbage-collecting blobs: manual delete, clear, and the retention job.

use super::{
    db::{History, Inner},
    queries::{delete_ids, is_referenced, load_guarded},
};
use crate::{
    blobs::Hash,
    error::Result,
    settings::{self, Settings},
};
use rusqlite::{Connection, params};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, SystemTime},
};
use uuid::Uuid;

/// Unreferenced blobs younger than this are left alone (an item may be about to reference them).
const ORPHAN_GRACE: Duration = Duration::from_secs(3600);
const STALE_PART: Duration = Duration::from_secs(24 * 3600);
const DAY_MS: u64 = 86_400_000;

/// Oldest unpinned item with a stored rep whose content no other item references and the live
/// outbox (`temp.guarded`) does not protect.
const OLDEST_WITH_OWN_BLOB: &str = "SELECT i.id FROM items i WHERE i.pinned = 0 AND EXISTS (
       SELECT 1 FROM reps r WHERE r.item_id = i.id AND r.present = 1
         AND NOT EXISTS (SELECT 1 FROM reps o WHERE o.hash = r.hash AND o.item_id <> i.id)
         AND NOT EXISTS (SELECT 1 FROM files f WHERE f.hash = r.hash)
         AND r.hash NOT IN (SELECT hash FROM temp.guarded))
     ORDER BY i.created_ms ASC, i.id ASC LIMIT 1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetentionReport {
    pub items_deleted: usize,
    pub blobs_removed: usize,
    pub bytes_freed: u64,
}

impl Inner {
    fn protected(&self) -> HashSet<Hash> {
        self.guard
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|f| f()))
            .unwrap_or_default()
    }

    /// Remove blobs no row and no guard references; `sizes` = stored sizes for the report.
    fn gc(
        &self,
        conn: &Connection,
        hashes: Vec<Hash>,
        sizes: &HashMap<Hash, u64>,
        rep: &mut RetentionReport,
    ) -> Result<()> {
        let protect = self.protected();
        for h in hashes.into_iter().collect::<HashSet<_>>() {
            if protect.contains(&h) || is_referenced(conn, &h)? {
                continue;
            }
            let existed = self.store.size_of(&h)?.is_some();
            self.store.remove(&h)?;
            if existed {
                rep.blobs_removed += 1;
                rep.bytes_freed += sizes.get(&h).copied().unwrap_or(0);
            }
        }
        Ok(())
    }

    fn delete_where(
        &self,
        conn: &mut Connection,
        sql: &str,
        p: impl rusqlite::Params,
        sizes: &HashMap<Hash, u64>,
        rep: &mut RetentionReport,
    ) -> Result<usize> {
        let tx = conn.transaction()?;
        let ids = tx
            .prepare(sql)?
            .query_map(p, |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let hashes = delete_ids(&tx, &ids)?;
        tx.commit()?;
        self.gc(conn, hashes, sizes, rep)?;
        rep.items_deleted += ids.len();
        Ok(ids.len())
    }

    fn retention(&self, conn: &mut Connection, now_ms: u64) -> Result<RetentionReport> {
        let s: Settings = settings::load(conn)?;
        let mut rep = RetentionReport::default();
        let sizes: HashMap<Hash, u64> = self
            .store
            .list()?
            .into_iter()
            .map(|b| (b.hash, b.size))
            .collect();
        let mut total: u64 = sizes.values().sum();
        let cutoff = now_ms.saturating_sub(u64::from(s.retention_days) * DAY_MS) as i64;
        self.delete_where(
            conn,
            "SELECT id FROM items WHERE pinned = 0 AND created_ms < ?1",
            [cutoff],
            &sizes,
            &mut rep,
        )?;
        total = total.saturating_sub(rep.bytes_freed);
        // Size cap: drop the oldest unpinned items that own stored bytes until the rest fit.
        // Items without a blob of their own (inline-only, or sharing content with another item)
        // would free nothing, so they are skipped, not deleted; so are blobs the outbox protects.
        load_guarded(conn, &self.protected())?;
        while total > s.retention_bytes {
            let before = rep.bytes_freed;
            let n = self.delete_where(conn, OLDEST_WITH_OWN_BLOB, [], &sizes, &mut rep)?;
            if n == 0 {
                break; // only pinned / shared / inline-only content left
            }
            total = total.saturating_sub(rep.bytes_freed - before);
        }
        self.sweep_orphans(conn, &mut rep)?;
        Ok(rep)
    }

    fn sweep_orphans(&self, conn: &Connection, rep: &mut RetentionReport) -> Result<()> {
        let protect = self.protected();
        let now = SystemTime::now();
        for b in self.store.list()? {
            let young = now
                .duration_since(b.modified)
                .is_ok_and(|a| a < ORPHAN_GRACE);
            if young || protect.contains(&b.hash) || is_referenced(conn, &b.hash)? {
                continue;
            }
            self.store.remove(&b.hash)?;
            rep.blobs_removed += 1;
            rep.bytes_freed += b.size;
        }
        self.store.sweep_stale_parts(STALE_PART)?;
        Ok(())
    }
}

impl History {
    /// Delete one item (pinned or not) and its unreferenced blobs. False if unknown.
    pub async fn delete_item(&self, id: Uuid) -> Result<bool> {
        let inner = self.0.clone();
        self.write(move |c| {
            let n = inner.delete_where(
                c,
                "SELECT id FROM items WHERE id = ?1",
                params![id.to_string()],
                &HashMap::new(),
                &mut RetentionReport::default(),
            )?;
            Ok(n > 0)
        })
        .await
    }

    /// Delete everything, or everything unpinned. Returns the number of items removed.
    pub async fn clear(&self, keep_pinned: bool) -> Result<usize> {
        let inner = self.0.clone();
        self.write(move |c| {
            inner.delete_where(
                c,
                "SELECT id FROM items WHERE pinned = 0 OR ?1 = 0",
                params![keep_pinned],
                &HashMap::new(),
                &mut RetentionReport::default(),
            )
        })
        .await
    }

    /// Age + size retention and orphan GC. Pinned items and guarded blobs are never touched.
    /// Run at start and hourly.
    pub async fn run_retention(&self, now_ms: u64) -> Result<RetentionReport> {
        let inner = self.0.clone();
        self.write(move |c| inner.retention(c, now_ms)).await
    }

    /// Same, for sync callers (a single indexed lookup on the reader connection).
    pub(crate) fn blob_referenced_sync(&self, hash: &Hash) -> Result<bool> {
        let g = self
            .0
            .r
            .lock()
            .map_err(|_| crate::error::CoreError::Store("history lock poisoned".into()))?;
        is_referenced(&g, hash)
    }

    /// Is any item still using this blob?
    pub async fn blob_referenced(&self, hash: Hash) -> Result<bool> {
        self.read(move |c| is_referenced(c, &hash)).await
    }
}
