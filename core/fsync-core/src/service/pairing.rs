//! Pairing, both sides. Each device always shows the SAS and must confirm it.

use super::{
    FsyncService,
    clips::now,
    delegate::Out,
    error::{FsyncError, SvcResult},
    inner::{AbortOnDrop, Inner, lock},
    types::{PairingEvent, PairingMode, PairingOffer, PairingPeer},
};
use crate::{
    error::{CoreError, PairError},
    pair::{DeviceInfo, PairSecret},
    sync::ApiListBackend,
};
use fsync_proto::pairing::QrPayload;
use std::{sync::Arc, time::Duration};
use tokio::sync::oneshot;
use zeroize::Zeroizing;

/// How long an offer (code or QR) waits for the other device.
const OFFER_TTL: Duration = Duration::from_secs(300);
/// How long `pairing_join` waits for the SAS to be ready.
const JOIN_WAIT: Duration = Duration::from_secs(60);
const RELAY_WAIT: Duration = Duration::from_secs(3);

impl Inner {
    /// Our relay URL for the offer; waits (bounded) for the relay connection.
    async fn wait_relay(&self, engine: &crate::engine::Engine) -> Option<String> {
        if self.cfg.relay_urls.is_empty() {
            return None;
        }
        engine.node().wait_online(RELAY_WAIT).await;
        engine.node().local_addrs().relay_url
    }

    pub(super) async fn pairing_offer(
        self: &Arc<Self>,
        mode: PairingMode,
    ) -> SvcResult<PairingOffer> {
        let engine = self.engine()?;
        // Before the backend call, so a refused offer does not burn a backend code.
        if engine.node().pairing_busy() {
            return Err(CoreError::Invalid("a pairing window is already open").into());
        }
        let relay = self.wait_relay(&engine).await;
        let node_id = engine.node().id();
        let (secret, offer) = match mode {
            PairingMode::Code => {
                let c = self.api.create_pairing(node_id, relay).await?;
                let offer = PairingOffer {
                    qr_payload: None,
                    code: Some(c.code.clone()),
                    expires_ms: c.expires_at,
                };
                (PairSecret::Code(Zeroizing::new(c.code)), offer)
            }
            PairingMode::Qr => {
                let mut s = [0u8; 16];
                getrandom::fill(&mut s).map_err(|e| FsyncError::Internal(e.to_string()))?;
                let qr = QrPayload {
                    node_id,
                    secret: s,
                    relay,
                };
                let offer = PairingOffer {
                    qr_payload: Some(qr.build()),
                    code: None,
                    expires_ms: now() + OFFER_TTL.as_millis() as u64,
                };
                (PairSecret::Qr(Zeroizing::new(s)), offer)
            }
        };
        let info = DeviceInfo {
            name: self.cfg.device_name.clone(),
            platform: self.cfg.platform,
            account_id: *lock(&self.account)?,
        };
        // Opened synchronously so "already open" reaches the caller; the guard inside
        // `pending` closes the window however the task ends.
        let pending = engine.node().open_pairing(secret, info, OFFER_TTL)?;
        let me = self.clone();
        let task = tokio::spawn(async move {
            // Success and protocol failures are reported by the core's own events.
            if let Err(CoreError::Pair(PairError::Timeout)) = pending.wait().await {
                me.emit(Out::Pairing(PairingEvent::Failed {
                    reason: "pairing offer expired".into(),
                }));
            }
        });
        lock(&self.pairing)?.task = Some(AbortOnDrop(task));
        Ok(offer)
    }

    pub(super) async fn pairing_join(self: &Arc<Self>, input: &str) -> SvcResult<PairingPeer> {
        let engine = self.engine()?;
        let input = input.trim();
        let (node, relay, secret) = if input.starts_with("fsync://") {
            let q = QrPayload::parse(input).map_err(CoreError::from)?;
            (
                q.node_id,
                q.relay.clone(),
                PairSecret::Qr(Zeroizing::new(q.secret)),
            )
        } else {
            let l = self.api.lookup_pairing(input).await?;
            (
                l.node_id,
                l.relay_url,
                PairSecret::Code(Zeroizing::new(input.to_owned())),
            )
        };
        let (tx, rx) = oneshot::channel();
        let me = self.clone();
        let backend = ApiListBackend {
            api: self.api.clone(),
            trust: self.trust.clone(),
        };
        {
            let mut slot = lock(&self.pairing)?;
            slot.join_waiter = Some(tx);
            let task = tokio::spawn(async move {
                let r = engine
                    .node()
                    .pair_with(&node, relay.as_deref(), &[], &secret, &backend)
                    .await;
                // An error before the SAS was shown goes to the waiting caller. Later failures
                // are reported by the core's `Failed` event.
                if let Err(e) = r
                    && let Ok(mut s) = lock(&me.pairing)
                    && let Some(w) = s.join_waiter.take()
                {
                    let _ = w.send(Err(e.into()));
                }
            });
            slot.task = Some(AbortOnDrop(task));
        }
        match tokio::time::timeout(JOIN_WAIT, rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err(FsyncError::Cancelled),
            Err(_) => {
                lock(&self.pairing)?.task = None;
                Err(FsyncError::Network("pairing timed out".into()))
            }
        }
    }
}

impl FsyncService {
    /// Shows this device to another one. `Code`: 6 digits via the backend; `Qr`: payload for
    /// a QR image (no backend call). The offer lives 5 minutes; progress arrives as pairing
    /// events (`ConfirmRequest` with the SAS, then `Completed` or `Failed`).
    pub fn pairing_offer(&self, mode: PairingMode) -> SvcResult<PairingOffer> {
        self.run(self.inner.pairing_offer(mode))
    }

    /// The existing device joins a new one by code or `fsync://pair?...` payload. Returns once
    /// both sides can compare the SAS; then call `pairing_confirm`.
    pub fn pairing_join(&self, input: &str) -> SvcResult<PairingPeer> {
        self.run(self.inner.pairing_join(input))
    }

    /// Answer to "do the words match?" on either side, for the `request_id` of the
    /// `PairingPeer` shown. Stale ids are refused. Nothing is trusted before both accept.
    pub fn pairing_confirm(&self, request_id: u64, accept: bool) -> SvcResult<()> {
        self.inner.pairing_confirm(request_id, accept)
    }

    /// Abandons the current offer / join: closes the window and rejects a pending confirm.
    pub fn pairing_cancel(&self) -> SvcResult<()> {
        self.inner.pairing_cancel()
    }
}
