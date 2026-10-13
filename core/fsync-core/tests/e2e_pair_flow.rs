//! Full flow against the REAL backend: login, pair/1 over iroh, list-change push.

mod backend_harness;
mod common;
use backend_harness::{Backend, login, start_presence, until};
use common::peer;
use fsync_core::{
    DeviceInfo, EngineEvent, PairSecret, PairingEvent,
    api::PresenceEvent,
    sync::{ApiListBackend, run_presence_pump},
};
use fsync_proto::{DeviceList, Platform, sign};
use std::time::Duration;
use zeroize::Zeroizing;

#[tokio::test]
async fn full_flow_login_pair_over_iroh_and_list_change_push() {
    let be = Backend::start().await;
    let (mut a, mut b) = (peer().await, peer().await);
    let (la, lb) = (login(&be).await, login(&be).await);
    assert_eq!(la.account, lb.account);

    // A: genesis on the backend + pinned locally, presence online.
    let g = a.genesis(la.account);
    la.api.put_device_list(&g, None).await.unwrap();
    let (_pa, rx_a) = start_presence(&la.api, &a);
    let (pump_sink, mut pump_events) = fsync_core::EventSink::channel();
    let a_pump = tokio::spawn(run_presence_pump(
        rx_a,
        la.api.clone(),
        a.trust.clone(),
        a.node.discovery.clone(),
        pump_sink,
    ));

    // B: offers itself (code), presence works because the offer is live.
    let offer = lb.api.create_pairing(b.nid(), None).await.unwrap();
    let (_pb, mut rx_b) = start_presence(&lb.api, &b);
    until(&mut rx_b, |e| {
        matches!(e, PresenceEvent::Connected).then_some(())
    })
    .await;
    let (b_sink, mut b_pump_events) = fsync_core::EventSink::channel();
    let b_pump = tokio::spawn(run_presence_pump(
        rx_b,
        lb.api.clone(),
        b.trust.clone(),
        b.node.discovery.clone(),
        b_sink,
    ));

    // A looks the code up, then pairs over iroh using the real backend for publish/chain.
    let found = la.api.lookup_pairing(&offer.code).await.unwrap();
    assert_eq!(found.node_id, b.nid());
    let backend = ApiListBackend {
        api: la.api.clone(),
        trust: a.trust.clone(),
    };
    let secret = PairSecret::Code(Zeroizing::new(offer.code.clone()));
    let info = DeviceInfo {
        name: "Device B".into(),
        platform: Platform::Win,
        account_id: Some(lb.account),
    };
    let addr = [b.loopback()];
    let (b_id, relay) = (found.node_id, found.relay_url);

    let (ev_a, ev_b) = (&mut a.events, &mut b.events);
    let new_side = b.node.await_pairing(
        PairSecret::Code(Zeroizing::new(offer.code.clone())),
        info,
        Duration::from_secs(30),
    );
    let existing = a
        .node
        .pair_with(&b_id, relay.as_deref(), &addr, &secret, &backend);
    let accept = async {
        loop {
            if let Some(EngineEvent::Pairing(PairingEvent::ConfirmRequest {
                sas, decision, ..
            })) = ev_a.recv().await
            {
                decision.accept();
                return sas;
            }
        }
    };
    let sas_b = async {
        loop {
            if let Some(EngineEvent::Pairing(PairingEvent::ConfirmRequest {
                sas, decision, ..
            })) = ev_b.recv().await
            {
                decision.accept();
                return sas;
            }
        }
    };
    let (rn, re, sas_a, sas_n) = tokio::time::timeout(Duration::from_secs(20), async {
        tokio::join!(new_side, existing, accept, sas_b)
    })
    .await
    .expect("pairing timed out");
    assert_eq!(rn.unwrap(), 1);
    assert_eq!(re.unwrap(), 1);
    assert_eq!(sas_a, sas_n);
    assert!(b.trust.is_trusted(&a.nid()) && a.trust.is_trusted(&b.nid()));

    // Backend holds seq 1 as published by A during pairing.
    assert_eq!(
        lb.api
            .get_device_list()
            .await
            .unwrap()
            .unwrap()
            .decode()
            .unwrap()
            .seq,
        1
    );

    // A adds a third device (seq 2) via PUT; B (connected to presence) is pushed DeviceListChanged,
    // syncs from the backend and pins seq 2.
    let c = peer().await;
    let head = a.trust.head().unwrap();
    let n2 = sign(
        a.id.signing_key(),
        &DeviceList::next_add(&head, a.nid(), c.nid(), "c".into(), Platform::Cli, 3).unwrap(),
    )
    .unwrap();
    la.api.put_device_list(&n2, Some(1)).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while b.trust.list().map(|l| l.seq) != Some(2) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("B must pin seq 2 after DeviceListChanged");
    assert!(b.trust.is_trusted(&c.nid()));
    // The pump reported no errors.
    while let Ok(e) = b_pump_events.try_recv() {
        assert!(!matches!(e, EngineEvent::Error { .. }), "{e:?}");
    }
    while let Ok(e) = pump_events.try_recv() {
        assert!(!matches!(e, EngineEvent::Error { .. }), "{e:?}");
    }
    a_pump.abort();
    b_pump.abort();
}
