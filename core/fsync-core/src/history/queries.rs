//! Write queries (run on the writer connection).

use super::{
    db::History,
    types::{FileState, NewItem, kind_code},
};
use crate::{
    blobs::Hash,
    error::{CoreError, Result},
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use uuid::Uuid;

const PREVIEW_CHARS: usize = 200;

fn db_int(v: u64) -> Result<i64> {
    i64::try_from(v).map_err(|_| CoreError::Invalid("value out of range"))
}

/// False if the id already exists (nothing changes).
pub(super) fn insert_item(tx: &Transaction<'_>, it: &NewItem) -> Result<bool> {
    let total = it
        .reps
        .iter()
        .map(|r| r.size)
        .chain(it.files.iter().map(|f| f.size))
        .try_fold(0u64, u64::checked_add)
        .ok_or(CoreError::Invalid("item size overflow"))?;
    let preview: Option<String> = it
        .preview
        .as_ref()
        .map(|p| p.chars().take(PREVIEW_CHARS).collect());
    let id = it.id.to_string();
    let n = tx.execute(
        "INSERT OR IGNORE INTO items(id, direction, peer, kind, created_ms, total_size, preview)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            it.direction.code(),
            it.peer.as_ref().map(|p| p.as_bytes().as_slice()),
            kind_code(it.kind),
            db_int(it.created_ms)?,
            db_int(total)?,
            preview
        ],
    )?;
    if n == 0 {
        return Ok(false);
    }
    for r in &it.reps {
        tx.execute(
            "INSERT INTO reps(item_id, idx, mime, size, hash, present) VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                id,
                r.idx,
                r.mime,
                db_int(r.size)?,
                r.hash.as_slice(),
                r.present
            ],
        )?;
    }
    for f in &it.files {
        tx.execute(
            "INSERT INTO files(item_id, file_id, name, rel_path, size, hash, local_path, state)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                id,
                f.file_id,
                f.name,
                f.rel_path,
                db_int(f.size)?,
                f.hash.as_slice(),
                f.local_path,
                f.state as i64
            ],
        )?;
    }
    Ok(true)
}

/// Blob hashes referenced by an item's reps and files.
pub(super) fn item_hashes(conn: &Connection, id: &str) -> Result<Vec<Hash>> {
    let mut st = conn.prepare(
        "SELECT hash FROM reps WHERE item_id = ?1 UNION SELECT hash FROM files WHERE item_id = ?1",
    )?;
    let rows = st.query_map([id], |r| r.get::<_, Vec<u8>>(0))?;
    let mut out = vec![];
    for h in rows {
        out.push(Hash::try_from(h?).map_err(|_| CoreError::Store("bad hash in history".into()))?);
    }
    Ok(out)
}

/// Delete items (rows cascade); returns the hashes they referenced.
pub(super) fn delete_ids(tx: &Transaction<'_>, ids: &[String]) -> Result<Vec<Hash>> {
    let mut hashes = vec![];
    for id in ids {
        hashes.extend(item_hashes(tx, id)?);
        tx.execute("DELETE FROM items WHERE id = ?1", [id])?;
    }
    Ok(hashes)
}

pub(super) fn is_referenced(conn: &Connection, h: &Hash) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM reps WHERE hash = ?1)
                 OR EXISTS(SELECT 1 FROM files WHERE hash = ?1)",
            [h.as_slice()],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(false))
}

impl History {
    /// Store an item with its reps and files. Returns false for sensitive items (never stored)
    /// and for ids already present.
    pub async fn record(&self, item: NewItem) -> Result<bool> {
        if item.sensitive {
            return Ok(false);
        }
        self.write(move |c| {
            let tx = c.transaction()?;
            let inserted = insert_item(&tx, &item)?;
            tx.commit()?;
            Ok(inserted)
        })
        .await
    }

    /// False if the item does not exist.
    pub async fn set_pinned(&self, id: Uuid, pinned: bool) -> Result<bool> {
        self.write(move |c| {
            let n = c.execute(
                "UPDATE items SET pinned = ?2 WHERE id = ?1",
                params![id.to_string(), pinned],
            )?;
            Ok(n > 0)
        })
        .await
    }

    /// The rep's bytes are now in the blob store.
    pub async fn mark_rep_present(&self, id: Uuid, idx: u32) -> Result<bool> {
        self.write(move |c| {
            let n = c.execute(
                "UPDATE reps SET present = 1 WHERE item_id = ?1 AND idx = ?2",
                params![id.to_string(), idx],
            )?;
            Ok(n > 0)
        })
        .await
    }

    /// The user saved the file at `path`.
    pub async fn set_file_local_path(&self, id: Uuid, file_id: u32, path: String) -> Result<bool> {
        self.write(move |c| {
            let n = c.execute(
                "UPDATE files SET local_path = ?3, state = ?4 WHERE item_id = ?1 AND file_id = ?2",
                params![id.to_string(), file_id, path, FileState::Saved as i64],
            )?;
            Ok(n > 0)
        })
        .await
    }
}

/// Mirrors blobs protected by the live outbox into `temp.guarded` so SQL can skip them.
pub(super) fn load_guarded(
    conn: &rusqlite::Connection,
    hashes: &std::collections::HashSet<crate::blobs::Hash>,
) -> crate::error::Result<()> {
    conn.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS guarded(hash BLOB PRIMARY KEY); DELETE FROM guarded;",
    )?;
    let mut ins = conn.prepare("INSERT OR IGNORE INTO guarded(hash) VALUES (?1)")?;
    for h in hashes {
        ins.execute([&h[..]])?;
    }
    Ok(())
}
