//! Head-only pairing pin (no chain from genesis) and self-removal detection.

use fsync_core::{
    EngineEvent, EventSink, Identity,
    trust::{Ingest, MemoryTrustStore, TrustManager, UNKNOWN_ROOT},
};
use fsync_proto::{DeviceList, Platform, SignedDeviceList, sign};
use std::sync::Arc;
use uuid::Uuid;

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

fn manager() -> (Arc<TrustManager>, tokio::sync::mpsc::Receiver<EngineEvent>) {
    let (sink, rx) = EventSink::channel();
    let tm = TrustManager::new(Arc::new(MemoryTrustStore::default()), sink).unwrap();
    (Arc::new(tm), rx)
}

#[test]
fn head_only_pin_checks_signer_account_members_and_signature() {
    let (tm, _rx) = manager();
    let (a, b) = (Identity::generate().unwrap(), Identity::generate().unwrap());
    let acct = Uuid::new_v4();
    let g = genesis(&a, acct);
    let l1 = add(&g, &a, &b);
    let (ida, idb) = (a.node_id(), b.node_id());
    let stranger = Identity::generate().unwrap().node_id();
    // Signer must be the pairing peer.
    assert!(tm.pin_paired_head(&l1, &idb, &stranger, None).is_err());
    // Must list both devices.
    assert!(tm.pin_paired_head(&l1, &stranger, &ida, None).is_err());
    // Account must match when known.
    assert!(
        tm.pin_paired_head(&l1, &idb, &ida, Some(Uuid::new_v4()))
            .is_err()
    );
    // Forged body.
    let mut forged = l1.clone();
    let last = forged.body.len() - 1;
    forged.body[last] ^= 1;
    assert!(tm.pin_paired_head(&forged, &idb, &ida, None).is_err());
    assert!(tm.head().is_none());

    assert_eq!(tm.pin_paired_head(&l1, &idb, &ida, Some(acct)).unwrap(), 1);
    assert_eq!(tm.root_hash(), Some(UNKNOWN_ROOT));
    assert!(
        tm.pin_paired_head(&l1, &idb, &ida, None).is_err(),
        "no re-pin"
    );

    // Successors verify against the head; a chain through our head teaches the root.
    let c = Identity::generate().unwrap();
    let l2 = add(&l1, &a, &c);
    assert_eq!(
        tm.ingest_chain(&[g.clone(), l1, l2]).unwrap(),
        Ingest::Advanced { seq: 2 }
    );
    assert_eq!(tm.root_hash(), Some(g.hash().unwrap()));
}

#[test]
fn removed_device_revokes_all_trust_and_emits_once() {
    let (tm, mut rx) = manager();
    let (a, b) = (Identity::generate().unwrap(), Identity::generate().unwrap());
    let g = genesis(&a, Uuid::new_v4());
    tm.pin_genesis(&g).unwrap();
    tm.set_self(b.node_id()).unwrap(); // pinned list lacks us
    assert!(matches!(rx.try_recv(), Ok(EngineEvent::SelfRemoved)));
    assert!(tm.is_self_removed() && !tm.is_trusted(&a.node_id()));

    // Added back: trust returns.
    let l1 = add(&g, &a, &b);
    tm.ingest(&l1).unwrap();
    assert!(!tm.is_self_removed() && tm.is_trusted(&a.node_id()));
    while rx.try_recv().is_ok() {}

    // Removed by A: event, nobody trusted (also not the remaining device).
    let rm = DeviceList::next_remove(&l1, a.node_id(), &b.node_id(), 3).unwrap();
    tm.ingest(&sign(a.signing_key(), &rm).unwrap()).unwrap();
    let mut got = 0;
    while let Ok(e) = rx.try_recv() {
        got += usize::from(matches!(e, EngineEvent::SelfRemoved));
    }
    assert_eq!(got, 1);
    assert!(!tm.is_trusted(&a.node_id()) && !tm.is_trusted(&b.node_id()));
}
