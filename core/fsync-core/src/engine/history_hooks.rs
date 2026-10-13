//! Recording sent/received items into history, and the hourly retention job.

use super::Shared;
use crate::{
    blobs::blocking,
    clip::now_ms,
    history::{Direction, History, NewItem},
};
use fsync_proto::{ClipOffer, NodeId};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::task::JoinHandle;
use uuid::Uuid;

const RETENTION_EVERY: Duration = Duration::from_secs(3600);

/// Aborts the task when dropped (engine shutdown).
pub(super) struct TaskGuard(JoinHandle<()>);

impl Drop for TaskGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Shared {
    /// History problems never fail clipboard sync; they surface as events (no content).
    fn history_failed(&self, what: &str, e: impl std::fmt::Display) {
        self.events.error(format!("history: {what}: {e}"));
    }

    /// Reps were persisted by `build_entry`; this adds the rows.
    pub(crate) async fn record_sent(&self, offer: &ClipOffer, peer: Option<NodeId>) {
        let Some(h) = &self.history else { return };
        let item = NewItem::from_offer(offer, Direction::Out, peer, false);
        if let Err(e) = h.record(item).await {
            self.history_failed("record sent", e);
        }
    }

    /// Inline reps are stored now (history must be able to restore them later); the rest
    /// become present once fetched.
    pub(crate) async fn record_received(&self, from: NodeId, offer: &ClipOffer) {
        // Sensitive offers are never recorded (nor their inline bytes put in the store).
        let Some(h) = self.history.as_ref().filter(|_| !offer.sensitive) else {
            return;
        };
        let (store, o) = (self.store.clone(), offer.clone());
        let put = blocking(move || {
            for (rep, inline) in o.reps.iter().zip(&o.inline) {
                if let Some(bytes) = inline {
                    store.put(&rep.hash, bytes)?;
                }
            }
            Ok(())
        })
        .await;
        if let Err(e) = put {
            self.history_failed("store inline reps", e);
        }
        let item = NewItem::from_offer(offer, Direction::In, Some(from), false);
        if let Err(e) = h.record(item).await {
            self.history_failed("record received", e);
        }
    }

    pub(crate) async fn note_rep_fetched(&self, item: Uuid, rep: u32) {
        if let Some(h) = &self.history
            && let Err(e) = h.mark_rep_present(item, rep).await
        {
            self.history_failed("mark rep", e);
        }
    }

    pub(crate) async fn note_file_saved(&self, item: Uuid, file_id: u32, path: &Path) {
        if let Some(h) = &self.history
            && let Err(e) = h
                .set_file_local_path(item, file_id, path.to_string_lossy().into_owned())
                .await
        {
            self.history_failed("file path", e);
        }
    }
}

/// Wire history into the engine: blobs the outbox serves are exempt from GC, and retention
/// runs now and hourly.
pub(super) fn start_retention(h: History, sh: &Arc<Shared>) -> TaskGuard {
    let weak = Arc::downgrade(sh);
    h.set_blob_guard(Arc::new(move || {
        weak.upgrade()
            .and_then(|s| s.outbox.lock().ok().map(|o| o.blob_hashes()))
            .unwrap_or_default()
    }));
    let events = sh.events.clone();
    TaskGuard(tokio::spawn(async move {
        let mut tick = tokio::time::interval(RETENTION_EVERY);
        loop {
            tick.tick().await;
            if let Err(e) = h.run_retention(now_ms()).await {
                events.error(format!("history: retention: {e}"));
            }
        }
    }))
}
