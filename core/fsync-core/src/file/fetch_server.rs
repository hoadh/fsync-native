//! Sender side of file/1: authorize, check the source is unchanged, stream verified-by-receiver data.

use super::served::Served;
use crate::{
    blobs::blocking,
    engine::Shared,
    error::{CoreError, Result},
    events::EngineEvent,
    net::{
        convert::node_id_of,
        frames::{recv_frame, send_frame},
    },
};
use fsync_proto::{FetchReq, FetchResp, FetchStatus, FetchTarget, NodeId};
use iroh::{
    endpoint::{Connection, SendStream},
    protocol::{AcceptError, ProtocolHandler},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;

const REQ_TIMEOUT: Duration = Duration::from_secs(10);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(30);
/// A peer that stops reading stalls our writes at most this long.
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
const BATCH: usize = 1 << 20;
/// Concurrent request handlers per connection; further streams wait in QUIC flow control.
pub(super) const MAX_STREAMS_PER_CONN: usize = 16;

#[derive(Clone)]
pub struct FileHandler(pub Arc<Shared>);

impl std::fmt::Debug for FileHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileHandler")
    }
}

impl ProtocolHandler for FileHandler {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let remote = node_id_of(&conn);
        let slots = Arc::new(Semaphore::new(MAX_STREAMS_PER_CONN));
        loop {
            // Take the slot first: while all are busy, new streams are not even accepted.
            let Ok(slot) = slots.clone().acquire_owned().await else {
                break;
            };
            let Ok((tx, rx)) = conn.accept_bi().await else {
                break;
            };
            let sh = self.0.clone();
            tokio::spawn(async move {
                let _slot = slot;
                if let Err(e) = serve(&sh, remote, tx, rx).await {
                    tracing::debug!("file/1 request ended: {e}");
                }
            });
        }
        Ok(())
    }
}

fn denied() -> (FetchResp, Option<Arc<Served>>) {
    let r = |status| FetchResp {
        status,
        size: 0,
        hash: [0; 32],
    };
    (r(FetchStatus::Denied), None)
}

impl Shared {
    /// Policy: requester trusted AND the item was offered to it; source unchanged.
    fn resolve(&self, req: &FetchReq, remote: &NodeId) -> (FetchResp, Option<Arc<Served>>) {
        let miss = |status| {
            (
                FetchResp {
                    status,
                    size: 0,
                    hash: [0; 32],
                },
                None,
            )
        };
        // Authorize before revealing whether an item exists.
        if !self.trust.is_trusted(remote) {
            return denied();
        }
        let found = self.outbox.lock().ok().and_then(|g| {
            let e = g.get(&req.item_id, Instant::now())?;
            let allowed = e.offered_to.contains(remote);
            let s = match req.target {
                FetchTarget::Rep(i) => e.reps.get(i as usize),
                FetchTarget::File(i) => e.files.get(i as usize),
            };
            Some((allowed, s.cloned()))
        });
        match found {
            None | Some((_, None)) => miss(FetchStatus::NotFound),
            Some((false, _)) => denied(),
            Some((true, Some(s))) if req.offset > s.size => miss(FetchStatus::NotFound),
            Some((true, Some(s))) if !s.unchanged() => miss(FetchStatus::Changed),
            Some((true, Some(s))) => (
                FetchResp {
                    status: FetchStatus::Ok,
                    size: s.size,
                    hash: s.hash,
                },
                Some(s),
            ),
        }
    }
}

async fn serve(
    sh: &Arc<Shared>,
    remote: NodeId,
    mut tx: SendStream,
    mut rx: iroh::endpoint::RecvStream,
) -> Result<()> {
    let req: FetchReq = recv_frame(&mut rx, REQ_TIMEOUT).await?;
    req.validate()?;
    let (resp, served) = sh.resolve(&req, &remote);
    send_frame(&mut tx, &resp).await?;
    if let Some(s) = &served {
        if !req.no_cvs {
            let cvs: Vec<u8> = s.cvs.iter().flatten().copied().collect();
            write_timed(&mut tx, &cvs).await?;
        }
        stream_data(sh, &mut tx, s.clone(), &req).await?;
    }
    tx.finish().map_err(CoreError::net)?;
    let drained = tokio::time::timeout(DRAIN_TIMEOUT, tx.stopped()).await;
    if let (Some(s), FetchTarget::File(file_id), Ok(Ok(_))) = (&served, req.target, drained) {
        let to_end = req.len == 0 || req.offset.saturating_add(req.len) >= s.size;
        if to_end {
            sh.events.emit(EngineEvent::FileServed {
                to: remote,
                item_id: req.item_id,
                file_id,
            });
        }
    }
    Ok(())
}

async fn stream_data(
    sh: &Arc<Shared>,
    tx: &mut SendStream,
    s: Arc<Served>,
    req: &FetchReq,
) -> Result<()> {
    let end = match req.len {
        0 => s.size,
        n => s.size.min(req.offset.saturating_add(n)),
    };
    let mut pos = req.offset;
    let mut buf = vec![0u8; BATCH];
    while pos < end {
        let want = usize::try_from(end - pos).unwrap_or(BATCH).min(BATCH);
        let (s2, store) = (s.clone(), sh.store.clone());
        let (b, n) = blocking(move || {
            let n = s2.read_at(store.as_ref(), pos, &mut buf[..want])?;
            Ok((buf, n))
        })
        .await?;
        buf = b;
        if n == 0 {
            // Source shrank under us: abort so the receiver never mistakes it for success.
            let _ = tx.reset(1u32.into());
            return Err(CoreError::Invalid("source shorter than announced"));
        }
        #[cfg(any(test, feature = "test-hooks"))]
        if let Some(hook) = &sh.cfg.serve_hook {
            hook(pos, &mut buf[..n]);
        }
        write_timed(tx, &buf[..n]).await?;
        pos += n as u64;
    }
    Ok(())
}

async fn write_timed(tx: &mut SendStream, data: &[u8]) -> Result<()> {
    tokio::time::timeout(WRITE_TIMEOUT, tx.write_all(data))
        .await
        .map_err(|_| CoreError::Transfer(crate::error::TransferError::Timeout))?
        .map_err(CoreError::net)
}
