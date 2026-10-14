//! Shared state behind `FsyncService`.

use super::{
    config::ServiceConfig,
    delegate::{Dispatcher, Out, ServiceDelegate},
    error::{FsyncError, SvcResult},
    transfers::Transfers,
    types::SessionTokens,
};
use crate::{
    api::{ApiClient, TokenCallback},
    blobs::{BlobStore, EncryptedBlobStore, FsBlobStore},
    engine::Engine,
    events::{EngineEvent, EventSink, PairDecision},
    history::History,
    identity::Identity,
    trust::{SqliteTrustStore, TrustManager},
};
use fsync_proto::{NodeId, api_dto::TokenPair};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};
use tokio::{sync::mpsc, sync::oneshot, task::JoinHandle};
use uuid::Uuid;

/// Aborts its task on drop.
pub(super) struct AbortOnDrop(pub JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Default)]
pub(super) struct Live {
    pub engine: Option<Arc<Engine>>,
    pub presence: Option<super::presence::PresenceRun>,
    pub tasks: Vec<AbortOnDrop>,
    /// Exclusive lock on `<data_dir>/engine.lock` while the engine runs (released on drop).
    pub data_lock: Option<std::fs::File>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct PeerSeen {
    pub online: bool,
    pub last_seen_ms: u64,
}

#[derive(Default)]
pub(super) struct PairingSlot {
    /// (request id, handle) of the confirmation currently shown to the user.
    pub decision: Option<(u64, PairDecision)>,
    pub next_request: u64,
    /// `pairing_join` waits here for the SAS.
    pub join_waiter: Option<oneshot::Sender<SvcResult<super::types::PairingPeer>>>,
    pub task: Option<AbortOnDrop>,
}

pub(super) struct Inner {
    pub cfg: ServiceConfig,
    pub identity: Identity,
    pub history: History,
    pub trust: Arc<TrustManager>,
    pub sink: EventSink,
    pub api: Arc<ApiClient>,
    pub out: Arc<Dispatcher>,
    pub store: Arc<dyn BlobStore>,
    pub account: Arc<Mutex<Option<Uuid>>>,
    pub live: Mutex<Live>,
    pub peers: Mutex<HashMap<NodeId, PeerSeen>>,
    pub xfers: Mutex<Transfers>,
    pub pairing: Mutex<PairingSlot>,
    /// Logout revocations still being retried (dropped with the service).
    pub revokes: Mutex<Vec<AbortOnDrop>>,
    /// First retry delay of a queued revocation.
    pub revoke_base: Mutex<std::time::Duration>,
}

pub(super) fn lock<T>(m: &Mutex<T>) -> SvcResult<MutexGuard<'_, T>> {
    m.lock()
        .map_err(|_| FsyncError::Internal("state lock poisoned".into()))
}

impl Inner {
    pub(super) fn open(
        cfg: ServiceConfig,
        delegate: Arc<dyn ServiceDelegate>,
    ) -> SvcResult<(Self, mpsc::Receiver<EngineEvent>)> {
        let identity = Identity::from_seed(&cfg.device_secret);
        let (sink, events) = EventSink::channel();
        let fs = Arc::new(FsBlobStore::open(cfg.data_dir.join("blobs"))?);
        let store: Arc<dyn BlobStore> = Arc::new(EncryptedBlobStore::new(fs, &cfg.history_key)?);
        let history = History::open(&cfg.data_dir, &cfg.history_key, store.clone(), &sink)?;
        let trust = Arc::new(TrustManager::new(
            Arc::new(SqliteTrustStore::new(&history)),
            sink.clone(),
        )?);
        let out = Arc::new(Dispatcher::spawn(delegate));
        let account: Arc<Mutex<Option<Uuid>>> = Arc::default();
        let on_tokens: TokenCallback = {
            let (out, account) = (out.clone(), account.clone());
            Arc::new(move |t: &TokenPair| {
                // During login the account id is not known yet; `login_verify` reports then.
                if let Some(id) = account.lock().ok().and_then(|a| *a) {
                    out.send(Out::Session(SessionTokens {
                        access: t.access.clone(),
                        refresh: t.refresh.clone(),
                        account_id: id.to_string(),
                    }));
                }
            })
        };
        let api = Arc::new(ApiClient::with_options(
            &cfg.api_base,
            None,
            Some(on_tokens),
            cfg.allow_insecure_http,
        )?);
        trust.set_self(identity.node_id())?;
        let inner = Self {
            cfg,
            identity,
            history,
            trust,
            sink,
            api,
            out,
            store,
            account,
            live: Mutex::default(),
            peers: Mutex::default(),
            xfers: Mutex::default(),
            pairing: Mutex::default(),
            revokes: Mutex::default(),
            revoke_base: Mutex::new(super::revoke::DEFAULT_BASE),
        };
        Ok((inner, events))
    }

    pub(super) fn me(&self) -> NodeId {
        self.identity.node_id()
    }

    pub(super) fn engine(&self) -> SvcResult<Arc<Engine>> {
        lock(&self.live)?
            .engine
            .clone()
            .ok_or_else(|| FsyncError::invalid("engine not started"))
    }

    pub(super) fn emit(&self, o: Out) {
        self.out.send(o);
    }
}
