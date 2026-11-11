//! Sender side of clip/2: build, register in the outbox, push one bi stream per peer and wait
//! for the receiver's app-level ack.

use super::{
    echo_guard::set_hash,
    input::{ClipInput, PublishOpts, PublishOutcome, Published},
    offer::build_entry,
    peer_send_lanes::{LaneRunner, Submit},
};
use crate::net::Dialer;
use crate::net::gate::CLOSE_UNTRUSTED;
use crate::{
    engine::Shared,
    error::{CoreError, Result},
};
use fsync_proto::{
    ALPN_CLIP, CLIP_ACK_ACCEPTED, CLIP_ACK_RATE_LIMITED, NodeId, frame::encode_frame,
};
use futures_util::stream::{FuturesUnordered, StreamExt};
use iroh::endpoint::{Connection, ConnectionError, WriteError};
use std::{
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use uuid::Uuid;

impl Shared {
    /// Paired devices other than us, narrowed to `targets` (which must all be paired).
    fn peers(&self, targets: Option<Vec<NodeId>>) -> Result<Vec<NodeId>> {
        let list = self
            .trust
            .list()
            .ok_or(CoreError::Invalid("no pinned device list"))?;
        let all = list
            .devices
            .iter()
            .map(|d| d.node_id)
            .filter(|n| *n != self.me);
        match targets {
            None => Ok(all.collect()),
            Some(t) if t.is_empty() => Err(CoreError::Invalid("no target devices")),
            Some(t) => {
                if let Some(bad) = t.iter().find(|n| !list.contains(n) || **n == self.me) {
                    return Err(CoreError::NotTrusted(bad.to_string()));
                }
                Ok(t)
            }
        }
    }

    /// Offers a clip to `opts.targets` (default: all paired peers) and waits for their acks, at
    /// most `cfg.publish_wait` unless it is a push or `wait_all`: peers that have not answered
    /// by then are reported `pending` and keep being tried in the background. A peer still busy
    /// with an older clip's attempt is `pending` at once (no wait at all).
    ///
    /// Publishes must be serialized for ordering: each one queues its clip in every peer's lane
    /// in turn, so two concurrent publishes can interleave there (A is newest for one peer, B for
    /// another) and "latest" then differs between peers. The apps publish one clip at a time;
    /// overlapping publishes are logged (debug), not prevented.
    pub(crate) async fn publish(
        self: &Arc<Self>,
        input: ClipInput,
        opts: PublishOpts,
    ) -> Result<PublishOutcome> {
        let _one = PublishInFlight::enter(self);
        let peers = self.peers(opts.targets)?;
        let sensitive = input.sensitive;
        let persist = self.history.is_some() && !sensitive;
        let mut entry = build_entry(
            input,
            &self.cfg,
            self.max_clip_bytes(),
            &self.store,
            self.me,
            opts.push,
            persist,
        )
        .await?;
        let o = &entry.offer;
        // An explicit push is the user's own action, never a clipboard echo of a received item.
        if !opts.push && self.echo.is_echo(&set_hash(&o.reps, &o.files)) {
            return Ok(PublishOutcome::Echo);
        }
        self.echo.clear();
        // Register before sending: a fast peer may fetch before the last send returns.
        entry.offered_to = peers.iter().copied().collect();
        let (offer, item_id) = (entry.offer.clone(), entry.offer.item_id);
        let freed = self
            .outbox
            .lock()
            .map_err(|_| CoreError::Closed)?
            .insert(entry, Instant::now());
        self.remove_blobs(freed);

        if persist {
            self.record_sent(&offer, (peers.len() == 1).then(|| peers[0]))
                .await;
        }
        let frame: Arc<[u8]> = encode_frame(&*offer)?.into();
        #[cfg(feature = "test-hooks")]
        self.track_frame(&frame);
        // Explicit pushes and one-shot callers wait for every peer's real result.
        let deadline = (!opts.push && !opts.wait_all)
            .then(|| tokio::time::Instant::now() + self.cfg.publish_wait);
        let mut answers = FuturesUnordered::new();
        for &peer in &peers {
            let (rx, busy) = self.start_send(peer, item_id, frame.clone(), opts.push);
            // Still busy with an older clip's attempt: slow or offline, so not worth the wait.
            if !(busy && deadline.is_some()) {
                answers.push(async move { (peer, rx.await) });
            }
        }
        let mut pending = peers;
        let (mut delivered, mut failed, mut refused) = (vec![], vec![], vec![]);
        loop {
            let next = match deadline {
                Some(d) => match tokio::time::timeout_at(d, answers.next()).await {
                    Ok(next) => next,
                    Err(_) => break,
                },
                None => answers.next().await,
            };
            let Some((peer, r)) = next else { break };
            if let Some(i) = pending.iter().position(|p| *p == peer) {
                pending.swap_remove(i);
            }
            // No answer at all: the send never ran or was aborted (engine closing).
            match r.unwrap_or(Err(CoreError::Closed)) {
                Ok(()) => delivered.push(peer),
                Err(CoreError::PeerRefused) => refused.push(peer),
                Err(_) => failed.push(peer),
            }
        }
        if !pending.is_empty() {
            tracing::debug!(
                "clip {item_id}: {} peer(s) still being tried in the background",
                pending.len()
            );
        }
        Ok(PublishOutcome::Sent(Published {
            item_id,
            delivered,
            failed,
            pending,
            refused,
        }))
    }

    /// Sends `frame` to `peer` in an engine-owned task (`Engine::close` aborts it) so a slow peer
    /// can finish after `publish` returned. The result arrives on the returned channel; when the
    /// send fails it is logged and the offer withdrawn, whether anyone still waits or not.
    /// Clipboard sends go through the peer's lane (one attempt at a time, latest wins: an older
    /// clip is never delivered after a newer one, see `PeerSendLanes`). A push bypasses the lane:
    /// it is the user's own action, so it neither waits behind nor supersedes clipboard sends.
    /// `busy`: the clip has to wait for an older attempt to this peer to end first.
    fn start_send(
        self: &Arc<Self>,
        peer: NodeId,
        item_id: Uuid,
        frame: Arc<[u8]>,
        push: bool,
    ) -> (oneshot::Receiver<Result<()>>, bool) {
        let (tx, rx) = oneshot::channel();
        let job = SendJob { item_id, frame, tx };
        let sh = self.clone();
        // After close the task is dropped unrun: `rx` then reports no answer.
        if push {
            self.background.spawn(async move {
                let r = sh.send_one(&peer, &job.frame).await;
                sh.finish_send(&peer, job, r);
            });
            return (rx, false);
        }
        match self.send_lanes.submit(peer, job) {
            Submit::Run(runner, job) => {
                self.background.spawn(sh.run_lane(peer, runner, job));
                (rx, false)
            }
            Submit::Wait { replaced } => {
                if let Some(old) = replaced {
                    let superseded = Err(CoreError::Net("superseded by a newer clip".into()));
                    self.finish_send(&peer, old, superseded);
                }
                (rx, true)
            }
        }
    }

    /// Sends `job`, then every clip that queued behind it, until the peer's lane is empty.
    async fn run_lane(self: Arc<Self>, peer: NodeId, mut lane: LaneRunner<SendJob>, job: SendJob) {
        let mut job = Some(job);
        while let Some(j) = job {
            let r = self.send_one(&peer, &j.frame).await;
            self.finish_send(&peer, j, r);
            job = lane.next();
        }
    }

    /// Logs a send's result, withdraws the offer from `peer` if it failed, and reports it.
    fn finish_send(&self, peer: &NodeId, job: SendJob, r: Result<()>) {
        let SendJob { item_id, tx, .. } = job;
        match &r {
            Ok(()) => tracing::debug!("clip {item_id} delivered to {peer}"),
            Err(e) => {
                tracing::debug!("clip {item_id} not delivered to {peer}: {e}");
                if let Ok(mut o) = self.outbox.lock() {
                    o.unoffer(&item_id, peer);
                }
            }
        }
        let _ = tx.send(r);
    }

    /// One attempt on the cached connection (short limit: a dead path must not hold up
    /// `publish`), then one on a fresh connection. Each attempt has a single deadline covering
    /// connect, send and ack, so one peer costs at most `FIRST_ATTEMPT + send_timeout` (7 s by
    /// default); `publish` stops waiting for it after `publish_wait` (except push / `wait_all`). "Delivered"
    /// means the peer accepted the offer (`CLIP_ACK_ACCEPTED`). A refusal (`PeerRefused`) on the
    /// cached connection is retried once too: that connection may predate a re-pairing.
    async fn send_one(&self, peer: &NodeId, frame: &[u8]) -> Result<()> {
        let dialer = self.dialer()?;
        let first = self.cfg.send_timeout.min(FIRST_ATTEMPT);
        match attempt(dialer, peer, frame, first).await {
            Err(CoreError::Net(_) | CoreError::PeerRefused) => {
                attempt(dialer, peer, frame, self.cfg.send_timeout).await
            }
            r => r,
        }
    }
}

/// One clip queued for one peer: its encoded offer and where its result goes.
pub(crate) struct SendJob {
    item_id: Uuid,
    frame: Arc<[u8]>,
    tx: oneshot::Sender<Result<()>>,
}

/// Counts running publishes so an overlap (unordered, see `Shared::publish`) shows in the log.
struct PublishInFlight<'a>(&'a Shared);

impl<'a> PublishInFlight<'a> {
    fn enter(sh: &'a Shared) -> Self {
        if sh.publishing.fetch_add(1, Ordering::SeqCst) > 0 {
            tracing::debug!(
                "overlapping clip publishes: per-peer order may not follow publish order"
            );
        }
        Self(sh)
    }
}

impl Drop for PublishInFlight<'_> {
    fn drop(&mut self) {
        self.0.publishing.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Limit for the attempt that may run on a cached (possibly half-dead) connection.
const FIRST_ATTEMPT: Duration = Duration::from_secs(2);

/// Connect, push the frame and wait for the receiver's ack byte under ONE deadline of `limit`. A
/// broken path closes and forgets the connection so the next try dials afresh. A peer that
/// answers "rate limited" or "rejected" is not retried and the connection stays. A connection the
/// peer closed with `CLOSE_UNTRUSTED` (it removed or reset us) is `PeerRefused`, not offline.
async fn attempt(dialer: &Dialer, peer: &NodeId, frame: &[u8], limit: Duration) -> Result<()> {
    let deadline = tokio::time::Instant::now() + limit;
    let timed_out = || CoreError::Net("send timed out".into());
    let conn = tokio::time::timeout_at(deadline, dialer.connect(peer, ALPN_CLIP))
        .await
        .map_err(|_| timed_out())??;
    let sent = tokio::time::timeout_at(deadline, push_and_wait_ack(&conn, frame))
        .await
        .unwrap_or_else(|_| Err(timed_out()));
    let sent = match sent {
        Err(CoreError::Net(_)) if refused_by_peer(&conn) => Err(CoreError::PeerRefused),
        r => r,
    };
    if matches!(sent, Err(CoreError::Net(_) | CoreError::PeerRefused)) {
        dialer.forget(peer, ALPN_CLIP);
        conn.close(0u32.into(), b"send failed");
    }
    sent
}

async fn push_and_wait_ack(conn: &Connection, frame: &[u8]) -> Result<()> {
    let (mut tx, mut rx) = conn.open_bi().await.map_err(CoreError::net)?;
    // A receiver that rate limits us stops the stream and then sends its ack: a stopped write or
    // finish is not a broken path, the ack byte is still readable (and says "rate limited").
    match tx.write_all(frame).await {
        Ok(()) => {
            let _ = tx.finish();
        }
        Err(WriteError::Stopped(_)) => {}
        Err(e) => return Err(CoreError::net(e)),
    }
    let mut ack = [0u8; 1];
    rx.read_exact(&mut ack).await.map_err(CoreError::net)?;
    match ack[0] {
        CLIP_ACK_ACCEPTED => Ok(()),
        CLIP_ACK_RATE_LIMITED => Err(CoreError::Invalid("peer is rate limiting us")),
        _ => Err(CoreError::Invalid("peer rejected the offer")),
    }
}

fn refused_by_peer(conn: &Connection) -> bool {
    matches!(
        conn.close_reason(),
        Some(ConnectionError::ApplicationClosed(c)) if c.error_code == CLOSE_UNTRUSTED.into()
    )
}
