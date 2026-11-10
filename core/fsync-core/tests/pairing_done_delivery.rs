//! pair/1 end on the new device (ADR 014, known issue "pairing connection lost").
//!
//! The new device used to emit `Completed` right after writing `Done`. A listener that shuts the
//! node down on `Completed` (the CLI `pair offer` exits) then cut `Done` off, and the existing
//! device failed with "network: connection lost" although both sides had the new list.
//! Now `Completed` follows the drain: the existing device closed (it has `Done`), or 5 s passed.
//! The tests use `InitiatorHooks` (feature `test-hooks`) to make the existing device slow or
//! silent after `Done`, so the ordering is checked deterministically.

mod common;
use common::{Echo, MemBackend, Peer, TEST_ALPN, pairing::new_info};
use fsync_core::{
    EngineEvent, EventSink, Identity, PairSecret, PairingEvent,
    net::Node,
    pair::InitiatorHooks,
    trust::{MemoryTrustStore, TrustManager},
};
use std::{
    cell::RefCell,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::mpsc::Receiver;
use uuid::Uuid;
use zeroize::Zeroizing;

async fn peer_with(hooks: InitiatorHooks) -> Peer {
    let id = Identity::generate().unwrap();
    let (sink, events) = EventSink::channel();
    let trust =
        Arc::new(TrustManager::new(Arc::new(MemoryTrustStore::default()), sink.clone()).unwrap());
    let node = Node::builder(id.clone(), trust.clone(), sink)
        .protocol(TEST_ALPN, Echo)
        .pair_hooks(hooks)
        .spawn()
        .await
        .unwrap();
    Peer {
        id,
        trust,
        node,
        events,
    }
}

fn secret() -> PairSecret {
    PairSecret::Code(Zeroizing::new("246810".to_owned()))
}

/// Accepts the SAS prompt, then returns at `Completed` (`None` on `Failed` or a closed channel).
async fn accept_then_completed(ev: &mut Receiver<EngineEvent>) -> Option<u64> {
    loop {
        match ev.recv().await? {
            EngineEvent::Pairing(PairingEvent::ConfirmRequest { decision, .. }) => {
                decision.accept()
            }
            EngineEvent::Pairing(PairingEvent::Completed { seq, .. }) => return Some(seq),
            EngineEvent::Pairing(PairingEvent::Failed { .. }) => return None,
            _ => {}
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn new_device_reports_completed_only_after_the_existing_device_has_done() {
    // E reads `Done`, then waits 300 ms before it reports and closes. Before the fix N's
    // `Completed` came during that wait, every time.
    let hold = Duration::from_millis(300);
    let mut e = peer_with(InitiatorHooks {
        after_done: hold,
        keep_open: false,
    })
    .await;
    let mut n = common::peer().await;
    let backend = MemBackend::with(e.genesis(Uuid::new_v4()));
    let (n_id, e_id, addr) = (n.nid(), e.nid(), [n.loopback()]);
    let pending = n
        .node
        .open_pairing(secret(), new_info(), Duration::from_secs(20))
        .unwrap();
    let s = secret();
    let e_events = RefCell::new(&mut e.events);
    let e_completed = RefCell::new(false);
    // E's events, polled without holding the borrow across an await.
    let poll_e = || {
        while let Ok(ev) = e_events.borrow_mut().try_recv() {
            match ev {
                EngineEvent::Pairing(PairingEvent::ConfirmRequest { decision, .. }) => {
                    decision.accept()
                }
                EngineEvent::Pairing(PairingEvent::Completed { .. }) => {
                    *e_completed.borrow_mut() = true
                }
                _ => {}
            }
        }
    };
    let drive_e = async {
        let end = Instant::now() + Duration::from_secs(15);
        while !*e_completed.borrow() && Instant::now() < end {
            poll_e();
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    };
    let new_side = async {
        let seq = accept_then_completed(&mut n.events).await;
        poll_e();
        let e_first = *e_completed.borrow();
        // Like the CLI: shut the node down the moment `Completed` arrives.
        n.node.close().await;
        (seq, e_first)
    };
    let e_side = e.node.pair_with(&n_id, None, &addr, &s, &backend);
    let ((n_seq, e_first), e_res, (), n_res) =
        tokio::time::timeout(Duration::from_secs(20), async {
            tokio::join!(new_side, e_side, drive_e, pending.wait())
        })
        .await
        .expect("pairing timed out");
    assert!(
        e_first,
        "new device reported Completed before the existing device had Done"
    );
    let seq = e_res.expect("existing device");
    assert_eq!(n_seq, Some(seq));
    assert_eq!(n_res.expect("new device outcome"), seq);
    assert!(n.trust.is_trusted(&e_id));
}

#[tokio::test(flavor = "multi_thread")]
async fn completed_comes_after_the_drain_cap_when_the_existing_device_never_closes() {
    let mut e = peer_with(InitiatorHooks {
        after_done: Duration::ZERO,
        keep_open: true,
    })
    .await;
    let mut n = common::peer().await;
    let backend = MemBackend::with(e.genesis(Uuid::new_v4()));
    let (n_id, addr) = (n.nid(), [n.loopback()]);
    let pending = n
        .node
        .open_pairing(secret(), new_info(), Duration::from_secs(20))
        .unwrap();
    let s = secret();
    let new_side = async {
        let seq = accept_then_completed(&mut n.events).await;
        (seq, Instant::now())
    };
    let e_side = async {
        let r = e.node.pair_with(&n_id, None, &addr, &s, &backend).await;
        (r, Instant::now())
    };
    let ((n_seq, n_at), (e_res, e_at), (), n_res) =
        tokio::time::timeout(Duration::from_secs(20), async {
            tokio::join!(
                new_side,
                e_side,
                common::pairing::answer_confirm(&mut e.events, true),
                pending.wait()
            )
        })
        .await
        .expect("pairing timed out");
    let seq = e_res.expect("existing device");
    assert_eq!(n_seq, Some(seq));
    assert_eq!(n_res.expect("new device outcome"), seq);
    let waited = n_at.duration_since(e_at);
    assert!(
        waited >= Duration::from_millis(4500) && waited <= Duration::from_millis(6500),
        "Completed {waited:?} after the existing device finished (cap 5 s)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_during_the_drain_still_reports_completed() {
    // E never closes, so N sits in the drain; N's node is shut down there.
    let mut e = peer_with(InitiatorHooks {
        after_done: Duration::ZERO,
        keep_open: true,
    })
    .await;
    let mut n = common::peer().await;
    let backend = MemBackend::with(e.genesis(Uuid::new_v4()));
    let (n_id, e_id, addr) = (n.nid(), e.nid(), [n.loopback()]);
    let pending = n
        .node
        .open_pairing(secret(), new_info(), Duration::from_secs(20))
        .unwrap();
    let s = secret();
    let e_side = e.node.pair_with(&n_id, None, &addr, &s, &backend);
    let (e_res, (), ()) = tokio::time::timeout(Duration::from_secs(20), async {
        tokio::join!(
            e_side,
            common::pairing::answer_confirm(&mut e.events, true),
            common::pairing::answer_confirm(&mut n.events, true)
        )
    })
    .await
    .expect("pairing timed out");
    let seq = e_res.expect("existing device");
    assert!(n.trust.is_trusted(&e_id), "new device pinned the list");
    let t0 = Instant::now();
    n.node.close().await;
    assert!(
        t0.elapsed() < Duration::from_secs(4),
        "close waited for the drain"
    );
    let n_seq = tokio::time::timeout(Duration::from_secs(2), accept_then_completed(&mut n.events))
        .await
        .expect("no Completed after shutdown");
    assert_eq!(n_seq, Some(seq));
    assert_eq!(pending.wait().await.expect("new device outcome"), seq);
}
