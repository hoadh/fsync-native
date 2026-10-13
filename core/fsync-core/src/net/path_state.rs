//! Per-peer connectivity state (Offline | Lan | Relay) derived from QUIC path events.

use super::convert::node_id_of;
use crate::events::{EngineEvent, EventSink, PathState};
use fsync_proto::NodeId;
use futures_util::StreamExt;
use iroh::endpoint::Connection;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

fn rank(s: PathState) -> u8 {
    match s {
        PathState::Offline => 0,
        PathState::Relay => 1,
        PathState::Lan => 2,
    }
}

/// Selected path of one connection.
pub fn connection_state(conn: &Connection) -> PathState {
    conn.paths()
        .iter()
        .find(|p| p.is_selected())
        .map_or(PathState::Offline, |p| {
            if p.is_ip() {
                PathState::Lan
            } else if p.is_relay() {
                PathState::Relay
            } else {
                PathState::Offline
            }
        })
}

#[derive(Default)]
struct Tables {
    /// peer -> (connection stable id -> state)
    conns: HashMap<NodeId, HashMap<usize, PathState>>,
    last: HashMap<NodeId, PathState>,
}

pub struct PathTracker {
    events: EventSink,
    tables: Mutex<Tables>,
}

impl PathTracker {
    pub fn new(events: EventSink) -> Arc<Self> {
        Arc::new(Self {
            events,
            tables: Mutex::default(),
        })
    }

    /// Best state across the peer's live connections.
    pub fn state(&self, peer: &NodeId) -> PathState {
        self.tables
            .lock()
            .ok()
            .and_then(|t| t.last.get(peer).copied())
            .unwrap_or(PathState::Offline)
    }

    fn update(&self, peer: NodeId, sid: usize, state: Option<PathState>) {
        let Ok(mut t) = self.tables.lock() else {
            return;
        };
        let m = t.conns.entry(peer).or_default();
        match state {
            Some(s) => {
                m.insert(sid, s);
            }
            None => {
                m.remove(&sid);
            }
        }
        let best = m
            .values()
            .copied()
            .max_by_key(|s| rank(*s))
            .unwrap_or(PathState::Offline);
        if m.is_empty() {
            t.conns.remove(&peer);
        }
        if t.last.insert(peer, best).unwrap_or(PathState::Offline) != best {
            self.events.emit(EngineEvent::DeviceStatus {
                node_id: peer,
                state: best,
            });
        }
    }

    /// Follow a connection until it closes.
    pub fn watch(self: &Arc<Self>, conn: Connection) {
        let me = Arc::clone(self);
        tokio::spawn(async move {
            let (peer, sid) = (node_id_of(&conn), conn.stable_id());
            me.update(peer, sid, Some(connection_state(&conn)));
            let mut ev = conn.path_events();
            loop {
                tokio::select! {
                    e = ev.next() => match e {
                        Some(_) => me.update(peer, sid, Some(connection_state(&conn))),
                        None => break,
                    },
                    _ = conn.closed() => break,
                }
            }
            me.update(peer, sid, None);
        });
    }
}
