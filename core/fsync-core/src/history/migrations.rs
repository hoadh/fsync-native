//! Embedded forward-only migrations, versioned by `PRAGMA user_version`.

use crate::error::{CoreError, Result};
use rusqlite::Connection;

const V1: &str = "
CREATE TABLE items(
  id TEXT PRIMARY KEY, direction INTEGER NOT NULL, peer BLOB, kind INTEGER NOT NULL,
  created_ms INTEGER NOT NULL, total_size INTEGER NOT NULL, preview TEXT,
  pinned INTEGER NOT NULL DEFAULT 0);
CREATE INDEX items_created ON items(created_ms DESC, id DESC);
CREATE TABLE reps(
  item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE, idx INTEGER NOT NULL,
  mime TEXT NOT NULL, size INTEGER NOT NULL, hash BLOB NOT NULL,
  present INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(item_id, idx));
CREATE INDEX reps_hash ON reps(hash);
CREATE TABLE files(
  item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE, file_id INTEGER NOT NULL,
  name TEXT NOT NULL, rel_path TEXT NOT NULL, size INTEGER NOT NULL, hash BLOB NOT NULL,
  local_path TEXT, state INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(item_id, file_id));
CREATE INDEX files_hash ON files(hash);
-- External-content FTS keyed on items.rowid: never VACUUM (it may renumber rowids).
CREATE VIRTUAL TABLE items_fts USING fts5(preview, content='items', content_rowid='rowid');
CREATE TRIGGER items_ai AFTER INSERT ON items BEGIN
  INSERT INTO items_fts(rowid, preview) VALUES (new.rowid, new.preview);
END;
CREATE TRIGGER items_ad AFTER DELETE ON items BEGIN
  INSERT INTO items_fts(items_fts, rowid, preview) VALUES ('delete', old.rowid, old.preview);
END;
CREATE TRIGGER items_au AFTER UPDATE OF preview ON items BEGIN
  INSERT INTO items_fts(items_fts, rowid, preview) VALUES ('delete', old.rowid, old.preview);
  INSERT INTO items_fts(rowid, preview) VALUES (new.rowid, new.preview);
END;
CREATE TABLE trust(seq INTEGER PRIMARY KEY, hash BLOB NOT NULL, signed_bytes BLOB NOT NULL);
CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
";

const MIGRATIONS: &[&str] = &[V1];

pub fn migrate(conn: &mut Connection) -> Result<()> {
    let current: usize = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if current > MIGRATIONS.len() {
        return Err(CoreError::Store(format!(
            "history schema v{current} is newer than this build (v{})",
            MIGRATIONS.len()
        )));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i + 1)?;
        tx.commit()?;
    }
    Ok(())
}
