//! Sender side of clip/1: build, register in the outbox, push one uni stream per peer.

use super::{
    echo_guard::set_hash,
    input::{ClipInput, PublishOpts, PublishOutcome, Published},
    offer::build_entry,
};
use crate::net::Dialer;
use crate::{
    engine::Shared,
    error::{CoreError, Result},
};
use fsync_proto::{ALPN_CLIP, NodeId, frame::encode_frame};
use std::time::{Duration, Instant};

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

    pub(crate) async fn publish(
        &self,
        input: ClipInput,
        opts: PublishOpts,
    ) -> Result<PublishOutcome> {
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
        let frame = encode_frame(&*offer)?;
        let results =
            futures_util::future::join_all(peers.iter().map(|p| self.send_one(p, &frame))).await;
        let (mut delivered, mut failed) = (vec![], vec![]);
        for (peer, r) in peers.into_iter().zip(results) {
            match r {
                Ok(()) => delivered.push(peer),
                Err(e) => {
                    tracing::debug!("clip offer not delivered: {e}");
                    if let Ok(mut o) = self.outbox.lock() {
                        o.unoffer(&item_id, &peer);
                    }
                    failed.push(peer);
                }
            }
        }
        Ok(PublishOutcome::Sent(Published {
            item_id,
            delivered,
            failed,
        }))
    }

    /// One attempt on the cached connection (short limit: a dead path must not hold up
    /// `publish`), then one on a fresh connection. Each attempt has a single deadline covering
    /// connect and send, so one peer costs at most `FIRST_ATTEMPT + send_timeout` (7 s by default),
    /// which also bounds `publish` because peers are sent to concurrently. "Delivered" means the peer's transport
    /// acknowledged the whole frame; it does not mean the peer accepted the offer (it may still
    /// drop it, e.g. when rate limited).
    async fn send_one(&self, peer: &NodeId, frame: &[u8]) -> Result<()> {
        let dialer = self.dialer()?;
        let first = self.cfg.send_timeout.min(FIRST_ATTEMPT);
        match attempt(dialer, peer, frame, first).await {
            Err(CoreError::Net(_)) => attempt(dialer, peer, frame, self.cfg.send_timeout).await,
            r => r,
        }
    }
}

/// Limit for the attempt that may run on a cached (possibly half-dead) connection.
const FIRST_ATTEMPT: Duration = Duration::from_secs(2);

/// Connect, push the frame and wait for the transport ack under ONE deadline of `limit`. A broken
/// path closes and forgets the connection so the next try dials afresh. A peer that refuses the
/// stream is throttling us: that is not retried and the connection stays.
async fn attempt(dialer: &Dialer, peer: &NodeId, frame: &[u8], limit: Duration) -> Result<()> {
    let deadline = tokio::time::Instant::now() + limit;
    let timed_out = || CoreError::Net("send timed out".into());
    let conn = tokio::time::timeout_at(deadline, dialer.connect(peer, ALPN_CLIP))
        .await
        .map_err(|_| timed_out())??;
    let sent = tokio::time::timeout_at(deadline, async {
        let mut tx = conn.open_uni().await.map_err(CoreError::net)?;
        tx.write_all(frame).await.map_err(CoreError::net)?;
        tx.finish().map_err(CoreError::net)?;
        // A caller that exits right after `publish` must not cut the connection mid-flight.
        match tx.stopped().await.map_err(CoreError::net)? {
            None => Ok(()),
            Some(_) => Err(CoreError::Invalid("peer refused the offer")),
        }
    })
    .await
    .unwrap_or_else(|_| Err(timed_out()));
    if matches!(sent, Err(CoreError::Net(_))) {
        dialer.forget(peer, ALPN_CLIP);
        conn.close(0u32.into(), b"send failed");
    }
    sent
}
