//! History and settings.

use super::{
    FsyncService,
    convert::parse_uuid,
    error::{FsyncError, SvcResult},
    types::{HistoryItem, HistoryPage},
};
use crate::{
    history::{Cursor, Direction},
    settings::Settings,
};
use uuid::Uuid;

const MAX_LIMIT: u32 = 200;

fn parse_cursor(s: &str) -> SvcResult<Cursor> {
    let (ms, id) = s
        .split_once(':')
        .ok_or_else(|| FsyncError::invalid("malformed cursor"))?;
    Ok(Cursor {
        created_ms: ms
            .parse()
            .map_err(|_| FsyncError::invalid("malformed cursor"))?,
        id: Uuid::parse_str(id).map_err(|_| FsyncError::invalid("malformed cursor"))?,
    })
}

fn cursor_string(c: &Cursor) -> String {
    format!("{}:{}", c.created_ms, c.id)
}

impl FsyncService {
    /// Newest first. A non-empty `query` searches previews and names.
    pub fn history(
        &self,
        query: Option<String>,
        cursor: Option<String>,
        limit: u32,
    ) -> SvcResult<HistoryPage> {
        let after = cursor.as_deref().map(parse_cursor).transpose()?;
        let limit = limit.clamp(1, MAX_LIMIT);
        let h = &self.inner.history;
        let page = self.run(async {
            Ok(
                match query.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
                    Some(q) => h.search(q, after, limit).await?,
                    None => h.list(after, limit).await?,
                },
            )
        })?;
        Ok(HistoryPage {
            items: page
                .items
                .into_iter()
                .map(|i| HistoryItem {
                    id: i.id.to_string(),
                    outgoing: i.direction == Direction::Out,
                    peer: i.peer.map(|p| p.to_string()),
                    kind: i.kind,
                    created_ms: i.created_ms,
                    total_size: i.total_size,
                    preview: i.preview,
                    pinned: i.pinned,
                })
                .collect(),
            next_cursor: page.next.as_ref().map(cursor_string),
        })
    }

    pub fn pin(&self, item_id: &str, pinned: bool) -> SvcResult<()> {
        let id = parse_uuid(item_id)?;
        self.run(async { found(self.inner.history.set_pinned(id, pinned).await?) })
    }

    pub fn delete(&self, item_id: &str) -> SvcResult<()> {
        let id = parse_uuid(item_id)?;
        self.run(async { found(self.inner.history.delete_item(id).await?) })
    }

    pub fn clear(&self, keep_pinned: bool) -> SvcResult<()> {
        self.run(async { Ok(self.inner.history.clear(keep_pinned).await.map(|_| ())?) })
    }

    pub fn settings(&self) -> SvcResult<Settings> {
        self.run(async { Ok(self.inner.history.settings().await?) })
    }

    /// Validates and stores; the size cap applies to the running engine immediately and the
    /// retention job runs once in the background.
    pub fn set_settings(&self, s: Settings) -> SvcResult<()> {
        let max = s.max_clip_bytes;
        self.run(async {
            self.inner.history.set_settings(s).await?;
            Ok(())
        })?;
        if let Ok(e) = self.inner.engine() {
            e.set_max_clip_bytes(max);
        }
        let h = self.inner.history.clone();
        self.rt_enter(|| {
            tokio::spawn(async move {
                if let Err(e) = h.run_retention(super::clips::now()).await {
                    tracing::debug!("retention after settings change failed: {e}");
                }
            });
            Ok(())
        })
    }
}

/// A pin/delete that touched no row names an item that does not exist.
fn found(hit: bool) -> SvcResult<()> {
    if hit {
        Ok(())
    } else {
        Err(FsyncError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_roundtrip_and_rejects_garbage() {
        let c = Cursor {
            created_ms: 42,
            id: Uuid::nil(),
        };
        assert_eq!(parse_cursor(&cursor_string(&c)).unwrap(), c);
        assert!(parse_cursor("nope").is_err());
        assert!(parse_cursor("x:00000000-0000-0000-0000-000000000000").is_err());
    }
}
