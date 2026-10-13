//! Backend presence: connect when signed in and listed, track who is online.

use super::{
    delegate::Out,
    error::SvcResult,
    inner::{AbortOnDrop, Inner, PeerSeen, lock},
    types::AccountEvent,
};
use crate::{
    api::{PresenceEvent, PresenceHandle, spawn_presence},
    engine::Engine,
    sync::run_presence_pump,
};
use fsync_proto::presence::{PeerInfo, ServerMsg};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Everything that makes up one presence session; dropping it stops all of it.
pub(super) struct PresenceRun {
    _conn: PresenceHandle,
    _tee: AbortOnDrop,
    _pump: AbortOnDrop,
}

impl Inner {
    /// (Re)starts presence if the engine runs, a session exists and this device is listed;
    /// otherwise stops it. A device that is not listed must never connect (the server would
    /// reject it and the client would retry in a tight loop).
    pub(super) fn restart_presence(self: &Arc<Self>) -> SvcResult<()> {
        let engine = lock(&self.live)?.engine.clone();
        let run = match engine {
            Some(e) if self.api.tokens().is_some() && self.trust.is_trusted(&self.me()) => {
                Some(self.spawn_presence(&e))
            }
            _ => None,
        };
        lock(&self.live)?.presence = run;
        Ok(())
    }

    /// Starts presence only if it is not running (see `restart_presence`).
    pub(super) fn ensure_presence(self: &Arc<Self>) -> SvcResult<()> {
        if lock(&self.live)?.presence.is_some() {
            return Ok(());
        }
        self.restart_presence()
    }

    fn spawn_presence(self: &Arc<Self>, engine: &Arc<Engine>) -> PresenceRun {
        let (tx, mut rx) = mpsc::channel::<PresenceEvent>(64);
        let (ptx, prx) = mpsc::channel::<PresenceEvent>(64);
        let feed = engine.node().addr_feed();
        let conn = spawn_presence(self.api.clone(), self.identity.clone(), feed, tx);
        let inner = self.clone();
        let tee = tokio::spawn(async move {
            while let Some(ev) = rx.recv().await {
                if matches!(ev, PresenceEvent::AuthExpired) {
                    inner.emit(Out::Account(AccountEvent::SessionExpired));
                    continue;
                }
                inner.note_presence(&ev);
                if ptx.send(ev).await.is_err() {
                    break;
                }
            }
        });
        let pump = tokio::spawn(run_presence_pump(
            prx,
            self.api.clone(),
            self.trust.clone(),
            engine.node().discovery.clone(),
            self.sink.clone(),
        ));
        PresenceRun {
            _conn: conn,
            _tee: AbortOnDrop(tee),
            _pump: AbortOnDrop(pump),
        }
    }

    fn note_presence(self: &Arc<Self>, ev: &PresenceEvent) {
        let changed: Vec<PeerInfo> = match ev {
            PresenceEvent::Server(ServerMsg::Peers { peers }) => peers.clone(),
            PresenceEvent::Server(ServerMsg::PeerUpdate(p)) => vec![p.clone()],
            _ => return,
        };
        let full = matches!(ev, PresenceEvent::Server(ServerMsg::Peers { .. }));
        let mut flipped = Vec::new();
        if let Ok(mut map) = lock(&self.peers) {
            if full {
                map.clear();
            }
            for p in changed {
                let new = PeerSeen {
                    online: p.online,
                    last_seen_ms: p.last_seen_ms,
                };
                if map
                    .insert(p.node_id, new)
                    .is_none_or(|o| o.online != new.online)
                {
                    flipped.push(p.node_id);
                }
            }
        }
        for id in flipped {
            self.emit_device(&id);
        }
    }
}
