//! Read queries: paging, full-text search, item detail.

use super::{
    db::History,
    types::{
        Cursor, Direction, FileRow, FileState, Item, ItemDetail, Page, RepRow, kind_from_code,
    },
};
use crate::{blobs::Hash, error::Result};
use fsync_proto::NodeId;
use rusqlite::{Connection, OptionalExtension, Row, params, types::Type};
use uuid::Uuid;

const MAX_PAGE: u32 = 500;
const COLS: &str =
    "i.id, i.direction, i.peer, i.kind, i.created_ms, i.total_size, i.preview, i.pinned";
const ORDER: &str = "ORDER BY i.created_ms DESC, i.id DESC LIMIT ?3";
const CURSOR: &str = "(?1 IS NULL OR (i.created_ms, i.id) < (?1, ?2))";

fn conv<E: std::error::Error + Send + Sync + 'static>(col: usize, e: E) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(col, Type::Blob, Box::new(e))
}

fn hash_col(r: &Row<'_>, col: usize) -> rusqlite::Result<Hash> {
    let v: Vec<u8> = r.get(col)?;
    Hash::try_from(v).map_err(|_| conv(col, std::fmt::Error))
}

fn item_row(r: &Row<'_>) -> rusqlite::Result<Item> {
    let id: String = r.get(0)?;
    let peer: Option<Vec<u8>> = r.get(2)?;
    Ok(Item {
        id: Uuid::parse_str(&id).map_err(|e| conv(0, e))?,
        direction: Direction::from_code(r.get(1)?),
        peer: peer
            .map(|p| <[u8; 32]>::try_from(p).map(NodeId))
            .transpose()
            .map_err(|_| conv(2, std::fmt::Error))?,
        kind: kind_from_code(r.get(3)?),
        created_ms: r.get::<_, i64>(4)? as u64,
        total_size: r.get::<_, i64>(5)? as u64,
        preview: r.get(6)?,
        pinned: r.get(7)?,
    })
}

/// Quote every word as an FTS5 prefix phrase so user input can never be a syntax error.
fn fts_query(q: &str) -> Option<String> {
    let terms: Vec<String> = q
        .split_whitespace()
        .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

fn page(c: &Connection, fts: Option<&str>, after: Option<Cursor>, limit: u32) -> Result<Page> {
    let limit = limit.clamp(1, MAX_PAGE);
    let sql = match fts {
        Some(_) => format!(
            "SELECT {COLS} FROM items_fts f JOIN items i ON i.rowid = f.rowid
             WHERE items_fts MATCH ?4 AND {CURSOR} {ORDER}"
        ),
        None => format!("SELECT {COLS} FROM items i WHERE ?4 IS NULL AND {CURSOR} {ORDER}"),
    };
    let (ms, id) = (
        after.map(|a| a.created_ms as i64),
        after.map(|a| a.id.to_string()),
    );
    let mut st = c.prepare(&sql)?;
    let rows = st.query_map(params![ms, id, limit + 1, fts], item_row)?;
    let mut items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let more = items.len() > limit as usize;
    items.truncate(limit as usize);
    let next = items.last().filter(|_| more).map(|i| Cursor {
        created_ms: i.created_ms,
        id: i.id,
    });
    Ok(Page { items, next })
}

fn detail(c: &Connection, id: Uuid) -> Result<Option<ItemDetail>> {
    let sid = id.to_string();
    let item = c
        .query_row(
            &format!("SELECT {COLS} FROM items i WHERE i.id = ?1"),
            [&sid],
            item_row,
        )
        .optional()?;
    let Some(item) = item else { return Ok(None) };
    let reps = c
        .prepare("SELECT idx, mime, size, hash, present FROM reps WHERE item_id = ?1 ORDER BY idx")?
        .query_map([&sid], |r| {
            Ok(RepRow {
                idx: r.get(0)?,
                mime: r.get(1)?,
                size: r.get::<_, i64>(2)? as u64,
                hash: hash_col(r, 3)?,
                present: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let files = c
        .prepare(
            "SELECT file_id, name, rel_path, size, hash, local_path, state
             FROM files WHERE item_id = ?1 ORDER BY file_id",
        )?
        .query_map([&sid], |r| {
            Ok(FileRow {
                file_id: r.get(0)?,
                name: r.get(1)?,
                rel_path: r.get(2)?,
                size: r.get::<_, i64>(3)? as u64,
                hash: hash_col(r, 4)?,
                local_path: r.get(5)?,
                state: if r.get::<_, i64>(6)? == 1 {
                    FileState::Saved
                } else {
                    FileState::Offered
                },
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Some(ItemDetail { item, reps, files }))
}

impl History {
    /// Newest first; pass the previous page's `next` to continue.
    pub async fn list(&self, after: Option<Cursor>, limit: u32) -> Result<Page> {
        self.read(move |c| page(c, None, after, limit)).await
    }

    /// Full-text search over previews (word prefixes, all words must match).
    pub async fn search(&self, query: &str, after: Option<Cursor>, limit: u32) -> Result<Page> {
        let Some(q) = fts_query(query) else {
            return Ok(Page::default());
        };
        self.read(move |c| page(c, Some(&q), after, limit)).await
    }

    pub async fn item(&self, id: Uuid) -> Result<Option<ItemDetail>> {
        self.read(move |c| detail(c, id)).await
    }
}
