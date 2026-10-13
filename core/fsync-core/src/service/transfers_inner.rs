//! `Inner` side of transfers: pulling files, cancelling.

use super::{
    delegate::Out,
    error::{FsyncError, SvcResult},
    inner::{AbortOnDrop, Inner, lock},
    transfers::{Recv, event},
    types::{TransferDirection, TransferEvent, TransferState},
};
use std::{path::PathBuf, sync::Arc, time::Duration};
use uuid::Uuid;

impl Inner {
    pub(super) fn emit_all(&self, evs: Vec<TransferEvent>) {
        for e in evs {
            self.emit(Out::Transfer(e));
        }
    }

    pub(super) fn fetch_file_to(
        self: &Arc<Self>,
        item: Uuid,
        file_id: u32,
        dest: PathBuf,
    ) -> SvcResult<u64> {
        let engine = self.engine()?;
        let offer = engine
            .received_offer(&item)
            .ok_or_else(|| FsyncError::invalid("unknown or expired item"))?;
        let f = offer
            .files
            .iter()
            .find(|f| f.file_id == file_id)
            .ok_or_else(|| FsyncError::invalid("no such file in item"))?;
        let (total, hash) = (f.size, f.hash);
        let mut x = lock(&self.xfers)?;
        let id = x.next_id();
        let mut started = event(id, item, TransferDirection::Receive, TransferState::Started);
        (started.file_id, started.bytes_total) = (Some(file_id), total);
        self.emit(Out::Transfer(started.clone()));
        let me = self.clone();
        let task = tokio::spawn(async move {
            let fetch = engine.fetch_file_to(item, file_id, &dest);
            tokio::pin!(fetch);
            let mut tick = tokio::time::interval(Duration::from_millis(250));
            let mut last = 0u64;
            let end = loop {
                tokio::select! {
                    r = &mut fetch => break r,
                    _ = tick.tick() => {
                        let got = me.store.part_len(&hash).unwrap_or(0);
                        if got != last {
                            last = got;
                            let mut p = started.clone();
                            (p.state, p.bytes_done) = (TransferState::Progress, got);
                            me.emit(Out::Transfer(p));
                        }
                    }
                }
            };
            let mut last_ev = started;
            match end {
                Ok(path) => {
                    (last_ev.state, last_ev.bytes_done) = (TransferState::Done, total);
                    last_ev.path = Some(path.to_string_lossy().into_owned());
                }
                Err(e) => {
                    last_ev.state = TransferState::Failed;
                    last_ev.message = Some(FsyncError::from(e).to_string());
                }
            }
            // Dropping our own entry must not abort this task mid-send: emit first.
            me.emit(Out::Transfer(last_ev));
            if let Ok(mut x) = lock(&me.xfers) {
                x.recvs.remove(&id);
            }
        });
        x.recvs.insert(
            id,
            Recv {
                item,
                file_id,
                total,
                _task: AbortOnDrop(task),
            },
        );
        Ok(id)
    }

    pub(super) fn cancel(&self, id: u64) -> SvcResult<()> {
        let ev = lock(&self.xfers)?.cancel(id);
        if let Some(e) = ev {
            self.emit(Out::Transfer(e));
        }
        Ok(())
    }
}
