//! Publishing clips, "send to" pushes, pulling content.

use super::{
    FsyncService,
    convert::{parse_node, parse_uuid},
    error::{FsyncError, SvcResult},
    inner::{Inner, lock},
    types::{LocalClip, Published},
    walk,
};
use crate::clip::{ClipInput, PublishOpts, PublishOutcome, RepInput};
use fsync_proto::ClipKind;
use std::{path::PathBuf, sync::Arc};

pub(super) fn now() -> u64 {
    crate::clip::now_ms()
}

fn kind_of(c: &LocalClip) -> SvcResult<ClipKind> {
    if !c.files.is_empty() {
        return Ok(ClipKind::Files);
    }
    let first = c
        .reps
        .first()
        .ok_or_else(|| FsyncError::invalid("clip has no content"))?;
    Ok(match first.mime.as_str() {
        "text/plain" => ClipKind::Text,
        m if m.starts_with("image/") => ClipKind::Image,
        _ => ClipKind::Rich,
    })
}

fn preview_of(c: &LocalClip, kind: ClipKind) -> Option<String> {
    let p = c.preview.clone().or_else(|| {
        let r = c.reps.first()?;
        (kind == ClipKind::Text).then(|| String::from_utf8_lossy(&r.data).into_owned())
    })?;
    Some(p.chars().take(200).collect())
}

/// Largest single `read_file_range` (bounds the buffer a caller can make us allocate).
pub(super) const MAX_RANGE_READ: u32 = 64 * 1024 * 1024;

fn outcome(o: PublishOutcome) -> Published {
    match o {
        PublishOutcome::Echo => Published {
            item_id: String::new(),
            echo: true,
            delivered: vec![],
            failed: vec![],
        },
        PublishOutcome::Sent(p) => Published {
            item_id: p.item_id.to_string(),
            echo: false,
            delivered: p.delivered.iter().map(|n| n.to_string()).collect(),
            failed: p.failed.iter().map(|n| n.to_string()).collect(),
        },
    }
}

impl Inner {
    async fn publish(self: &Arc<Self>, c: LocalClip) -> SvcResult<Published> {
        let kind = kind_of(&c)?;
        let files = if c.files.is_empty() {
            vec![]
        } else {
            let paths = c.files.clone();
            tokio::task::spawn_blocking(move || walk::expand(&paths))
                .await
                .map_err(|e| FsyncError::Internal(e.to_string()))??
        };
        let preview = preview_of(&c, kind);
        let input = ClipInput {
            kind,
            reps: c
                .reps
                .into_iter()
                .map(|r| RepInput {
                    mime: r.mime,
                    bytes: r.data,
                })
                .collect(),
            files,
            preview,
            sensitive: c.sensitive,
        };
        let o = self
            .engine()?
            .publish_clip(input, PublishOpts::default())
            .await?;
        Ok(outcome(o))
    }

    async fn send_files(self: &Arc<Self>, node: &str, paths: Vec<String>) -> SvcResult<u64> {
        let node = parse_node(node)?;
        if !self.trust.is_trusted(&node) {
            return Err(FsyncError::NotPaired);
        }
        let files = tokio::task::spawn_blocking(move || walk::expand(&paths))
            .await
            .map_err(|e| FsyncError::Internal(e.to_string()))??;
        let sizes = files
            .iter()
            .map(|f| std::fs::metadata(&f.path).map(|m| m.len()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| FsyncError::Io(e.to_string()))?;
        let opts = PublishOpts {
            targets: Some(vec![node]),
            push: true,
        };
        let sent = self
            .engine()?
            .publish_clip(ClipInput::files(files), opts)
            .await?;
        let PublishOutcome::Sent(p) = sent else {
            // Explicit pushes skip the echo guard, so this is not an offline peer.
            return Err(FsyncError::Internal(
                "push was suppressed as an echo".into(),
            ));
        };
        if !p.delivered.contains(&node) {
            return Err(FsyncError::PeerOffline);
        }
        let (id, evs) = lock(&self.xfers)?.register_send(p.item_id, sizes);
        self.emit_all(evs);
        Ok(id)
    }
}

impl FsyncService {
    pub fn publish_clip(&self, clip: LocalClip) -> SvcResult<Published> {
        self.run(self.inner.publish(clip))
    }

    /// Sends files/folders to one paired device ("Send to..."); progress and completion arrive
    /// as `Send` transfer events for the returned id. `PeerOffline` if it did not take the offer.
    pub fn send_files(&self, node_id: &str, paths: Vec<String>) -> SvcResult<u64> {
        self.run(self.inner.send_files(node_id, paths))
    }

    /// Bytes of one representation of a received item.
    pub fn fetch_rep(&self, item_id: &str, idx: u32) -> SvcResult<Vec<u8>> {
        let (engine, id) = (self.inner.engine()?, parse_uuid(item_id)?);
        self.run(async move { Ok(engine.fetch_rep(id, idx).await?) })
    }

    /// Pulls a received file into `dest_dir` (never overwrites). Returns immediately with a
    /// transfer id; progress and the final path arrive as `Receive` transfer events.
    pub fn fetch_file_to(&self, item_id: &str, file_id: u32, dest_dir: &str) -> SvcResult<u64> {
        let id = parse_uuid(item_id)?;
        self.rt_enter(|| {
            self.inner
                .fetch_file_to(id, file_id, PathBuf::from(dest_dir))
        })
    }

    /// Verified bytes of a remote file at `offset` (blocking; up to `len`, 30 s deadline).
    pub fn read_file_range(
        &self,
        item_id: &str,
        file_id: u32,
        offset: u64,
        len: u32,
    ) -> SvcResult<Vec<u8>> {
        let (engine, id) = (self.inner.engine()?, parse_uuid(item_id)?);
        let len = usize::try_from(len.min(MAX_RANGE_READ)).unwrap_or(0);
        self.run(async move { Ok(engine.read_file_range(id, file_id, offset, len).await?) })
    }

    /// Stops a running transfer (a pulled file keeps its partial data for resume).
    pub fn cancel(&self, transfer_id: u64) -> SvcResult<()> {
        self.inner.cancel(transfer_id)
    }
}

#[cfg(test)]
mod tests;
