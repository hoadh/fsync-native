//! TrustManager pending-reset lifecycle: wrong-account refusal, newest root replaces stale ones,
//! declines stick, and replacing the pin with a fresh genesis.

use fsync_core::{
    CoreError, EngineEvent, EventSink, Identity,
    trust::{Ingest, MemoryTrustStore, TrustManager},
};
use fsync_proto::{DeviceList, Platform, ProtoError, SignedDeviceList, sign};
use std::sync::Arc;
use uuid::Uuid;

struct Acct {
    a: Identity,
    b: Identity,
    id: Uuid,
}

fn genesis(who: &Identity, id: Uuid) -> SignedDeviceList {
    let l = DeviceList::genesis(id, who.node_id(), "d".into(), Platform::Cli, 1);
    sign(who.signing_key(), &l).unwrap()
}

fn add(prev: &SignedDeviceList, signer: &Identity, new: &Identity) -> SignedDeviceList {
    let l = DeviceList::next_add(
        prev,
        signer.node_id(),
        new.node_id(),
        "n".into(),
        Platform::Mac,
        2,
    )
    .unwrap();
    sign(signer.signing_key(), &l).unwrap()
}

fn setup() -> (
    Acct,
    Arc<TrustManager>,
    tokio::sync::mpsc::Receiver<EngineEvent>,
) {
    let acct = Acct {
        a: Identity::generate().unwrap(),
        b: Identity::generate().unwrap(),
        id: Uuid::new_v4(),
    };
    let (sink, rx) = EventSink::channel();
    let tm = Arc::new(TrustManager::new(Arc::new(MemoryTrustStore::default()), sink).unwrap());
    (acct, tm, rx)
}

#[test]
fn reset_from_another_account_is_refused() {
    let (ac, tm, _rx) = setup();
    let g = genesis(&ac.a, ac.id);
    tm.pin_genesis(&g).unwrap();
    let stranger = Identity::generate().unwrap();
    let foreign = genesis(&stranger, Uuid::new_v4());
    assert!(matches!(
        tm.ingest(&foreign),
        Err(CoreError::Proto(ProtoError::WrongAccount))
    ));
    assert!(tm.pending_reset_roots().is_empty());
}

#[test]
fn a_new_root_replaces_the_stale_pending_ones() {
    let (ac, tm, mut rx) = setup();
    tm.pin_genesis(&genesis(&ac.a, ac.id)).unwrap();
    let mut heads = vec![];
    for _ in 0..10 {
        let fresh = Identity::generate().unwrap();
        match tm.ingest(&genesis(&fresh, ac.id)).unwrap() {
            Ingest::ResetPending { head_hash } => heads.push(head_hash),
            other => panic!("a new root is never dropped: {other:?}"),
        }
    }
    // Only the latest root is pending; the stale ids are gone.
    let pending: Vec<_> = tm
        .pending_reset_infos()
        .iter()
        .map(|r| r.head_hash)
        .collect();
    assert_eq!(pending, vec![heads[9]]);
    for stale in &heads[..9] {
        assert_eq!(tm.confirm_reset(stale).unwrap(), None);
        assert!(!tm.decline_reset(stale).unwrap());
    }
    assert_eq!(
        tm.pending_reset_infos().len(),
        1,
        "stale answers change nothing"
    );
    assert_eq!(tm.confirm_reset(&heads[9]).unwrap(), Some(0));
    let mut prompts = 0;
    while let Ok(e) = rx.try_recv() {
        prompts += usize::from(matches!(e, EngineEvent::TrustReset { .. }));
    }
    assert_eq!(prompts, 10, "one announcement per root, none repeated");
}

#[test]
fn declined_root_is_forgotten_and_not_offered_again() {
    let (ac, tm, mut rx) = setup();
    let g = genesis(&ac.a, ac.id);
    tm.pin_genesis(&g).unwrap();
    let fresh = Identity::generate().unwrap();
    let new_root = genesis(&fresh, ac.id);
    let Ingest::ResetPending { head_hash } = tm.ingest(&new_root).unwrap() else {
        panic!()
    };
    let infos = tm.pending_reset_infos();
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].head_hash, head_hash);
    assert_eq!(infos[0].device_names, vec!["d".to_string()]);
    assert!(!tm.decline_reset(&[9; 32]).unwrap(), "unknown head");
    assert!(tm.decline_reset(&head_hash).unwrap());
    assert!(tm.is_declined_head(&head_hash));
    assert!(!tm.decline_reset(&head_hash).unwrap(), "already answered");
    // The old pin is untouched, and the same root neither returns nor prompts again.
    assert!(tm.is_trusted(&ac.a.node_id()) && !tm.is_trusted(&fresh.node_id()));
    assert_eq!(tm.ingest(&new_root).unwrap(), Ingest::Unchanged);
    assert!(tm.pending_reset_roots().is_empty());
    assert_eq!(tm.confirm_reset(&head_hash).unwrap(), None);
    let mut prompts = 0;
    while let Ok(e) = rx.try_recv() {
        prompts += usize::from(matches!(e, EngineEvent::TrustReset { .. }));
    }
    assert_eq!(prompts, 1);
    // A different root is still offered.
    let other = genesis(&Identity::generate().unwrap(), ac.id);
    assert!(matches!(
        tm.ingest(&other).unwrap(),
        Ingest::ResetPending { .. }
    ));
}

#[test]
fn replace_with_genesis_works_without_a_pin_and_clears_pending() {
    let (ac, tm, _rx) = setup();
    let fresh = Identity::generate().unwrap();
    let g2 = genesis(&fresh, ac.id);
    // No pin yet (a device that only logged in): allowed.
    tm.replace_with_genesis(&g2).unwrap();
    assert_eq!(tm.root_hash(), Some(g2.hash().unwrap()));
    // With a pin: replaces it, drops pending resets, wrong accounts and non-genesis are refused.
    tm.ingest(&genesis(&ac.a, ac.id)).unwrap();
    assert_eq!(tm.pending_reset_roots().len(), 1);
    let g3 = genesis(&ac.b, ac.id);
    tm.replace_with_genesis(&g3).unwrap();
    assert!(tm.pending_reset_roots().is_empty());
    assert!(tm.is_trusted(&ac.b.node_id()) && !tm.is_trusted(&fresh.node_id()));
    assert!(matches!(
        tm.replace_with_genesis(&genesis(&ac.a, Uuid::new_v4())),
        Err(CoreError::Proto(ProtoError::WrongAccount))
    ));
    assert!(tm.replace_with_genesis(&add(&g3, &ac.b, &ac.a)).is_err());
}
