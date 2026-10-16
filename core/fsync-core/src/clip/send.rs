//! Sender side of clip/2: build, register in the outbox, push one bi stream per peer and wait
//! for the receiver's app-level ack.

use super::{
    echo_guard::set_hash,
    input::{ClipInput, PublishOpts, PublishOutcome, Published},
    offer::build_entry,
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
use iroh::endpoint::{Connection, ConnectionError};
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
        let (mut delivered, mut failed, mut refused) = (vec![], vec![], vec![]);
        for (peer, r) in peers.into_iter().zip(results) {
            if let Err(e) = &r {
                tracing::debug!("clip offer not delivered: {e}");
                if let Ok(mut o) = self.outbox.lock() {
                    o.unoffer(&item_id, &peer);
                }
            }
            match r {
                Ok(()) => delivered.push(peer),
                Err(CoreError::PeerRefused) => refused.push(peer),
                Err(_) => failed.push(peer),
            }
        }
        Ok(PublishOutcome::Sent(Published {
            item_id,
            delivered,
            failed,
            refused,
        }))
    }

    /// One attempt on the cached connection (short limit: a dead path must not hold up
    /// `publish`), then one on a fresh connection. Each attempt has a single deadline covering
    /// connect, send and ack, so one peer costs at most `FIRST_ATTEMPT + send_timeout` (7 s by
    /// default), which also bounds `publish` because peers are sent to concurrently. "Delivered"
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
    tx.write_all(frame).await.map_err(CoreError::net)?;
    tx.finish().map_err(CoreError::net)?;
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
