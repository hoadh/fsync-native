//! In-memory presence hub. `InMemoryHub` is single-process; a multi-instance impl (Postgres
//! LISTEN/NOTIFY) can replace it behind the trait.

pub use super::hub_api::{CLOSE_UNAUTHORIZED, ConnId, Outbound, PresenceHub};
use super::hub_entry::{Account, Entry, broadcast, now_u64};
use fsync_proto::{
    NodeId,
    presence::{Addrs, PeerInfo, ServerMsg},
};
use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Default)]
pub struct InMemoryHub {
    accounts: Mutex<HashMap<Uuid, Account>>,
    next_conn: AtomicU64,
}

impl InMemoryHub {
    fn with<R>(&self, f: impl FnOnce(&mut HashMap<Uuid, Account>) -> R) -> R {
        f(&mut self.accounts.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

impl PresenceHub for InMemoryHub {
    fn connect(
        &self,
        account: Uuid,
        node: NodeId,
        pending: bool,
        sid: Uuid,
        tx: mpsc::Sender<Outbound>,
    ) -> ConnId {
        let conn = self.next_conn.fetch_add(1, Ordering::Relaxed);
        self.with(|m| {
            let acct = m.entry(account).or_default();
            let addrs = acct.get(&node).map(|e| e.addrs.clone()).unwrap_or_default();
            let entry = Entry {
                conn,
                sid,
                tx: Some(tx),
                pending,
                addrs,
                last_seen_ms: now_u64(),
            };
            let info = entry.info(node);
            acct.insert(node, entry);
            broadcast(acct, &ServerMsg::PeerUpdate(info), Some(&node), false);
        });
        conn
    }

    fn set_addrs(&self, account: Uuid, node: NodeId, conn: ConnId, addrs: Addrs) {
        self.with(|m| {
            let Some(acct) = m.get_mut(&account) else {
                return;
            };
            let Some(e) = acct.get_mut(&node).filter(|e| e.conn == conn) else {
                return;
            };
            e.addrs = addrs;
            e.last_seen_ms = now_u64();
            let info = e.info(node);
            broadcast(acct, &ServerMsg::PeerUpdate(info), Some(&node), false);
        });
    }

    fn disconnect(&self, account: Uuid, node: NodeId, conn: ConnId) {
        self.with(|m| {
            let Some(acct) = m.get_mut(&account) else {
                return;
            };
            let Some(e) = acct.get_mut(&node).filter(|e| e.conn == conn) else {
                return; // superseded by a newer connection, or removed from the list
            };
            e.tx = None;
            e.last_seen_ms = now_u64();
            let (info, pending) = (e.info(node), e.pending);
            if pending {
                acct.remove(&node); // unlisted nodes are not remembered
            }
            broadcast(acct, &ServerMsg::PeerUpdate(info), Some(&node), false);
            if acct.is_empty() {
                m.remove(&account);
            }
        });
    }

    fn snapshot(&self, account: Uuid, me: &NodeId, listed: &[NodeId]) -> Vec<PeerInfo> {
        self.with(|m| {
            let acct = m.get(&account);
            let mut out: Vec<PeerInfo> = listed
                .iter()
                .filter(|n| *n != me)
                .map(|n| match acct.and_then(|a| a.get(n)) {
                    Some(e) => e.info(*n),
                    None => PeerInfo {
                        node_id: *n,
                        online: false,
                        addrs: Addrs::default(),
                        last_seen_ms: 0,
                    },
                })
                .collect();
            if let Some(acct) = acct {
                out.extend(
                    acct.iter()
                        .filter(|(n, e)| *n != me && e.tx.is_some() && !listed.contains(n))
                        .map(|(n, e)| e.info(*n)),
                );
            }
            out
        })
    }

    fn device_list_changed(&self, account: Uuid, seq: u64, listed: &[NodeId]) {
        self.with(|m| {
            let Some(acct) = m.get_mut(&account) else {
                return;
            };
            let removed: Vec<NodeId> = acct
                .iter()
                .filter(|(n, e)| !e.pending && !listed.contains(n))
                .map(|(n, _)| *n)
                .collect();
            for n in removed {
                if let Some(mut e) = acct.remove(&n) {
                    e.close(CLOSE_UNAUTHORIZED, "removed");
                }
            }
            for (n, e) in acct.iter_mut() {
                e.pending &= !listed.contains(n); // a paired node is now a full member
            }
            broadcast(acct, &ServerMsg::DeviceListChanged { seq }, None, true);
        });
    }

    fn close_session(&self, account: Uuid, sid: Uuid) {
        self.with(|m| {
            if let Some(acct) = m.get_mut(&account) {
                for e in acct.values_mut().filter(|e| e.sid == sid) {
                    e.close(CLOSE_UNAUTHORIZED, "session ended");
                }
            }
        });
    }

    fn close_account(&self, account: Uuid) {
        self.with(|m| {
            if let Some(mut acct) = m.remove(&account) {
                for e in acct.values_mut() {
                    e.close(CLOSE_UNAUTHORIZED, "account deleted");
                }
            }
        });
    }
}
