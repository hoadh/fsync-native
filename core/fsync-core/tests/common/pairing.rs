//! Drives one pair/1 session between two in-process peers.
use super::{MemBackend, Peer};
use fsync_core::{DeviceInfo, EngineEvent, PairSecret, PairingEvent, Result};
use fsync_proto::Platform;
use std::time::Duration;
use uuid::Uuid;

/// Only bounds the wait for a peer that proves the secret; failed handshakes run into it.
const TTL: Duration = Duration::from_secs(3);

pub struct Outcome {
    pub existing: Result<u64>,
    pub new: Result<u64>,
    pub sas_existing: Option<String>,
    pub sas_new: Option<String>,
}

/// E (has the account) pairs with N; both users decide `accept`.
pub async fn pair(
    e: &mut Peer,
    n: &mut Peer,
    se: PairSecret,
    sn: PairSecret,
    accept: bool,
) -> (Outcome, MemBackend) {
    pair_decisions(e, n, se, sn, accept, accept).await
}

/// Like `pair` with separate answers from E's user and N's user.
pub async fn pair_decisions(
    e: &mut Peer,
    n: &mut Peer,
    se: PairSecret,
    sn: PairSecret,
    accept_e: bool,
    accept_n: bool,
) -> (Outcome, MemBackend) {
    let account = Uuid::new_v4();
    let backend = MemBackend::with(e.genesis(account));
    let out = pair_on(e, n, se, sn, accept_e, accept_n, account, &backend).await;
    (out, backend)
}

/// One session against an existing backend (E already has the account pinned).
#[allow(clippy::too_many_arguments)]
pub async fn pair_on(
    e: &mut Peer,
    n: &mut Peer,
    se: PairSecret,
    sn: PairSecret,
    accept_e: bool,
    accept_n: bool,
    account: Uuid,
    backend: &MemBackend,
) -> Outcome {
    let info = DeviceInfo {
        name: "New Phone".into(),
        platform: Platform::Mac,
        account_id: Some(account),
    };
    let n_id = n.nid();
    let addr = [n.loopback()];
    let (ev_e, ev_n) = (&mut e.events, &mut n.events);
    let new_fut = n.node.await_pairing(sn, info, TTL);
    let e_fut = e.node.pair_with(&n_id, None, &addr, &se, backend);
    let drive_e = async {
        let r = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match ev_e.recv().await {
                    Some(EngineEvent::Pairing(PairingEvent::ConfirmRequest {
                        sas,
                        decision,
                        name,
                        ..
                    })) => {
                        assert_eq!(name, "New Phone");
                        if accept_e {
                            decision.accept()
                        } else {
                            decision.reject()
                        }
                        return Some(sas);
                    }
                    Some(_) => {}
                    None => return None,
                }
            }
        })
        .await;
        r.ok().flatten()
    };
    let drive_n = async {
        let r = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match ev_n.recv().await? {
                    EngineEvent::Pairing(PairingEvent::ConfirmRequest {
                        sas,
                        decision,
                        name,
                        ..
                    }) => {
                        // N sees who is adding it.
                        assert_eq!(name, "first");
                        if accept_n {
                            decision.accept()
                        } else {
                            decision.reject()
                        }
                        return Some(sas);
                    }
                    EngineEvent::Pairing(PairingEvent::Failed { .. }) => return None,
                    _ => {}
                }
            }
        })
        .await;
        r.ok().flatten()
    };
    let (new, existing, sas_existing, sas_new) = tokio::join!(new_fut, e_fut, drive_e, drive_n);
    Outcome {
        existing,
        new,
        sas_existing,
        sas_new,
    }
}

/// Wait for the next `ConfirmRequest` and accept it (`accept`) or drop it unanswered.
pub async fn answer_confirm(ev: &mut tokio::sync::mpsc::Receiver<EngineEvent>, accept: bool) {
    while let Some(e) = ev.recv().await {
        if let EngineEvent::Pairing(PairingEvent::ConfirmRequest { decision, .. }) = e {
            if accept {
                decision.accept();
            }
            return;
        }
    }
}

pub fn new_info() -> DeviceInfo {
    DeviceInfo {
        name: "N".into(),
        platform: Platform::Mac,
        account_id: None,
    }
}
