//! TrustManager reset binding: confirm pins exactly the announced head, declines survive junk,
//! the device's own reset is never announced to itself.

use fsync_core::{
    EngineEvent, EventSink, Identity,
    trust::{Ingest, MemoryTrustStore, TrustManager},
};
use fsync_proto::{DeviceList, Platform, SignedDeviceList, sign};
use std::sync::Arc;
use uuid::Uuid;

fn genesis(who: &Identity, id: Uuid) -> SignedDeviceList {
    let l = DeviceList::genesis(id, who.node_id(), "d".into(), Platform::Cli, 1);
    sign(who.signing_key(), &l).unwrap()
}

fn add(prev: &SignedDeviceList, signer: &Identity, new: &Identity, name: &str) -> SignedDeviceList {
    let l = DeviceList::next_add(
        prev,
        signer.node_id(),
        new.node_id(),
        name.into(),
        Platform::Mac,
        2,
    )
    .unwrap();
    sign(signer.signing_key(), &l).unwrap()
}

fn setup() -> (
    Identity,
    Uuid,
    TrustManager,
    tokio::sync::mpsc::Receiver<EngineEvent>,
) {
    let (a, id) = (Identity::generate().unwrap(), Uuid::new_v4());
    let (sink, rx) = EventSink::channel();
    let tm = TrustManager::new(Arc::new(MemoryTrustStore::default()), sink).unwrap();
    tm.pin_genesis(&genesis(&a, id)).unwrap();
    (a, id, tm, rx)
}

fn prompts(rx: &mut tokio::sync::mpsc::Receiver<EngineEvent>) -> Vec<([u8; 32], Vec<String>)> {
    let mut out = vec![];
    while let Ok(e) = rx.try_recv() {
        if let EngineEvent::TrustReset {
            head_hash,
            device_names,
        } = e
        {
            out.push((head_hash, device_names));
        }
    }
    out
}

#[test]
fn a_grown_offer_is_announced_again_and_the_old_id_is_dead() {
    let (_a, id, tm, mut rx) = setup();
    let (c, d) = (Identity::generate().unwrap(), Identity::generate().unwrap());
    let root = genesis(&c, id);
    let Ingest::ResetPending { head_hash: h0 } = tm.ingest(&root).unwrap() else {
        panic!()
    };
    let l1 = add(&root, &c, &d, "phone");
    let Ingest::ResetPending { head_hash: h1 } =
        tm.ingest_chain(&[root.clone(), l1.clone()]).unwrap()
    else {
        panic!()
    };
    assert_ne!(h0, h1);
    let seen = prompts(&mut rx);
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[1], (h1, vec!["d".to_string(), "phone".to_string()]));
    // The user saw the one-device list: that answer must not pin the two-device one.
    assert_eq!(tm.confirm_reset(&h0).unwrap(), None);
    assert!(!tm.decline_reset(&h0).unwrap());
    assert!(!tm.is_trusted(&d.node_id()), "nothing pinned");
    assert_eq!(tm.pending_reset_infos().len(), 1);
    assert_eq!(tm.confirm_reset(&h1).unwrap(), Some(1));
    assert!(tm.is_trusted(&d.node_id()));
}

#[test]
fn a_declined_root_stays_declined_when_its_chain_grows_and_junk_cannot_evict_it() {
    let (_a, id, tm, mut rx) = setup();
    let (c, d) = (Identity::generate().unwrap(), Identity::generate().unwrap());
    let root = genesis(&c, id);
    let Ingest::ResetPending { head_hash } = tm.ingest(&root).unwrap() else {
        panic!()
    };
    assert!(tm.decline_reset(&head_hash).unwrap());
    // Plenty of junk roots; each replaces the previous one.
    for _ in 0..100 {
        tm.ingest(&genesis(&Identity::generate().unwrap(), id))
            .unwrap();
    }
    let l1 = add(&root, &c, &d, "phone");
    assert_eq!(
        tm.ingest_chain(&[root.clone(), l1.clone()]).unwrap(),
        Ingest::Unchanged
    );
    assert!(
        tm.is_declined_head(&l1.hash().unwrap()),
        "latest head remembered"
    );
    assert_eq!(tm.ingest(&root).unwrap(), Ingest::Unchanged);
    assert_eq!(
        prompts(&mut rx).len(),
        101,
        "the declined root once, each of the 100 junk roots once, nothing after the decline"
    );
}

#[test]
fn own_reset_is_not_announced_to_itself() {
    let (_a, id, tm, mut rx) = setup();
    let me = Identity::generate().unwrap();
    let mine = genesis(&me, id);
    {
        let _guard = tm.begin_own_reset().unwrap();
        assert_eq!(tm.ingest(&mine).unwrap(), Ingest::Unchanged);
        assert!(tm.pending_reset_roots().is_empty());
        tm.replace_with_genesis(&mine).unwrap();
    }
    assert!(prompts(&mut rx).is_empty());
    assert_eq!(tm.ingest(&mine).unwrap(), Ingest::Unchanged);
    // Once the guard is gone a foreign root is announced again.
    let other = genesis(&Identity::generate().unwrap(), id);
    assert!(matches!(
        tm.ingest(&other).unwrap(),
        Ingest::ResetPending { .. }
    ));
    assert_eq!(prompts(&mut rx).len(), 1);
}

#[test]
fn overlapping_own_resets_suppress_until_the_last_guard_drops() {
    let (_a, id, tm, mut rx) = setup();
    let first = tm.begin_own_reset().unwrap();
    let second = tm.begin_own_reset().unwrap();
    drop(first);
    let foreign = genesis(&Identity::generate().unwrap(), id);
    assert_eq!(tm.ingest(&foreign).unwrap(), Ingest::Unchanged);
    drop(second);
    assert!(matches!(
        tm.ingest(&foreign).unwrap(),
        Ingest::ResetPending { .. }
    ));
    assert_eq!(prompts(&mut rx).len(), 1);
}
