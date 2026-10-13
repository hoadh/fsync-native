//! Glue: backend presence + device-list sync feeding trust and discovery.

use crate::{
    api::{ApiClient, PresenceEvent},
    error::{CoreError, Result},
    events::EventSink,
    net::Discovery,
    pair::ListBackend,
    trust::{Ingest, TrustManager},
};
use fsync_proto::{ProtoError, SignedDeviceList, presence::ServerMsg};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Fetch lists newer than the pin and ingest them; falls back to the full chain when the
/// incremental one does not connect (reset, pruned history).
pub async fn sync_device_list(api: &ApiClient, trust: &TrustManager) -> Result<Ingest> {
    let from = trust
        .head()
        .ok_or(CoreError::Invalid("no pinned list; pair first"))?
        .decode()?
        .seq;
    match trust.ingest_chain(&api.chain_all(from).await?) {
        Err(CoreError::Proto(
            ProtoError::Gap { .. }
            | ProtoError::Rollback { .. }
            | ProtoError::SignerNotTrusted
            | ProtoError::PrevHashMismatch
            | ProtoError::TrustReset,
        )) => trust.ingest_chain(&api.chain_all(0).await?),
        other => other,
    }
}

/// `ListBackend` over the real API + trust manager (used by the pairing initiator).
pub struct ApiListBackend {
    pub api: Arc<ApiClient>,
    pub trust: Arc<TrustManager>,
}

impl ListBackend for ApiListBackend {
    async fn publish(&self, list: &SignedDeviceList, prev_seq: u64) -> Result<()> {
        self.api
            .put_device_list(list, Some(prev_seq))
            .await
            .map(|_| ())
    }
    async fn sync(&self) -> Result<()> {
        sync_device_list(&self.api, &self.trust).await.map(|_| ())
    }
    async fn full_chain(&self) -> Result<Vec<SignedDeviceList>> {
        self.api.chain_all(0).await
    }
}

/// Consume presence events until the channel closes.
pub async fn run_presence_pump(
    mut rx: mpsc::Receiver<PresenceEvent>,
    api: Arc<ApiClient>,
    trust: Arc<TrustManager>,
    discovery: Discovery,
    events: EventSink,
) {
    while let Some(ev) = rx.recv().await {
        match ev {
            PresenceEvent::Server(ServerMsg::Peers { peers }) => discovery.apply_peers(&peers),
            PresenceEvent::Server(ServerMsg::PeerUpdate(p)) => discovery.apply_peer(&p),
            // Not paired yet (e.g. push races our own pairing): the pairing chain covers it.
            PresenceEvent::Server(ServerMsg::DeviceListChanged { .. })
                if trust.head().is_none() => {}
            PresenceEvent::Server(ServerMsg::DeviceListChanged { .. }) => {
                match sync_device_list(&api, &trust).await {
                    Ok(Ingest::Advanced { .. }) => discovery.refresh(),
                    Ok(_) => {}
                    Err(e) => events.error(format!("device list sync failed: {e}")),
                }
            }
            PresenceEvent::AuthExpired => events.error("session expired: log in again"),
            PresenceEvent::Server(ServerMsg::Challenge { .. })
            | PresenceEvent::Connected
            | PresenceEvent::Disconnected => {}
        }
    }
}
