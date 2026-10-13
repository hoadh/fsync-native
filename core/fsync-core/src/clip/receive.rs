//! Receiver side of clip/1: validate, dedupe, arm the echo guard, tell the app.

use super::{
    echo_guard::set_hash,
    received_log::{ReceivedOffer, checked_total},
};
use crate::{
    engine::Shared,
    error::{CoreError, Result},
    events::{EngineEvent, OfferRef},
    net::convert::node_id_of,
};
use fsync_proto::{
    ClipKind, ClipOffer, NodeId,
    frame::{MAX_FRAME_LEN, decode_frame},
};
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use std::{sync::Arc, time::Duration};

const READ_TIMEOUT: Duration = Duration::from_secs(10);

impl Shared {
    async fn on_offer(&self, from: NodeId, raw: &[u8]) -> Result<()> {
        let (offer, used) = decode_frame::<ClipOffer>(raw)?;
        if used != raw.len() {
            return Err(CoreError::Invalid("trailing bytes after offer"));
        }
        if offer.origin != from {
            return Err(CoreError::Invalid("offer origin is not the sender"));
        }
        offer.validate()?;
        let total = checked_total(&offer.reps)?;
        let max = self.max_clip_bytes();
        if total > max {
            return Err(CoreError::TooLarge { size: total, max });
        }
        let offer = Arc::new(offer);
        let gone;
        {
            let mut log = self.log.lock().map_err(|_| CoreError::Closed)?;
            if !log.first_time(offer.item_id) {
                return Ok(()); // duplicate delivery
            }
            self.echo.mark_applied(set_hash(&offer.reps, &offer.files));
            gone = log.keep(Arc::new(ReceivedOffer {
                from,
                offer: offer.clone(),
            }));
        }
        if !gone.is_empty()
            && let Ok(mut readers) = self.readers.lock()
        {
            readers.evict_items(&gone);
        }
        // History first, so a UI reacting to the event finds the row.
        self.record_received(from, &offer).await;
        let o = OfferRef(offer.clone());
        self.events
            .emit(if offer.push && offer.kind == ClipKind::Files {
                EngineEvent::IncomingFiles { from, offer: o }
            } else {
                EngineEvent::RemoteClip { from, offer: o }
            });
        Ok(())
    }
}

#[derive(Clone)]
pub struct ClipHandler(pub Arc<Shared>);

impl std::fmt::Debug for ClipHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClipHandler")
    }
}

impl ProtocolHandler for ClipHandler {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let from = node_id_of(&conn);
        while let Ok(mut rx) = conn.accept_uni().await {
            if !self.0.offer_rate.allow(from) {
                tracing::debug!("offer from {from} dropped: rate limit");
                let _ = rx.stop(0u32.into());
                continue;
            }
            let sh = self.0.clone();
            tokio::spawn(async move {
                let read =
                    tokio::time::timeout(READ_TIMEOUT, rx.read_to_end(MAX_FRAME_LEN + 4)).await;
                let res = match read {
                    Ok(Ok(raw)) => sh.on_offer(from, &raw).await,
                    _ => Err(CoreError::Net("offer stream failed".into())),
                };
                if let Err(e) = res {
                    // Never include content; the peer id suffices to debug.
                    sh.events
                        .error(format!("rejected clip offer from {from}: {e}"));
                }
            });
        }
        Ok(())
    }
}
