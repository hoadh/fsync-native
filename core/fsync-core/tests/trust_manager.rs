//! TrustManager: pinning, rollback protection, reset confirmation + dedupe, persistence.

use fsync_core::{
    CoreError, EngineEvent, EventSink, Identity,
    trust::{FileTrustStore, Ingest, MemoryTrustStore, TrustManager},
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

fn seq(tm: &TrustManager) -> u64 {
    tm.list().unwrap().seq
}

#[test]
fn pins_highest_seq_and_rejects_rollback_and_forgery() {
    let (ac, tm, _rx) = setup();
    let g = genesis(&ac.a, ac.id);
    assert!(
        !tm.is_trusted(&ac.a.node_id()),
        "nothing pinned -> nobody trusted"
    );
    tm.pin_genesis(&g).unwrap();
    assert!(tm.is_trusted(&ac.a.node_id()) && !tm.is_trusted(&ac.b.node_id()));
    assert!(tm.pin_genesis(&g).is_err(), "cannot re-pin");

    let l1 = add(&g, &ac.a, &ac.b);
    assert_eq!(tm.ingest(&l1).unwrap(), Ingest::Advanced { seq: 1 });
    assert!(tm.is_trusted(&ac.b.node_id()));
    assert_eq!(tm.ingest(&l1).unwrap(), Ingest::Unchanged);

    // Withheld-removal / rollback attempt: the old list must not unpin seq 1.
    assert!(matches!(
        tm.ingest(&g),
        Ok(Ingest::Unchanged) | Err(CoreError::Proto(ProtoError::Rollback { .. }))
    ));
    assert_eq!(seq(&tm), 1);
    // Forged: signed by a non-member.
    let evil = Identity::generate().unwrap();
    let forged = add(&l1, &evil, &evil);
    assert!(tm.ingest(&forged).is_err());
    // Gap.
    let l2 = add(&l1, &ac.a, &Identity::generate().unwrap());
    let l3 = add(&l2, &ac.a, &Identity::generate().unwrap());
    assert!(matches!(
        tm.ingest(&l3),
        Err(CoreError::Proto(ProtoError::Gap { .. }))
    ));
    // Chain closes the gap.
    assert_eq!(
        tm.ingest_chain(&[l1.clone(), l2, l3]).unwrap(),
        Ingest::Advanced { seq: 3 }
    );
}

#[test]
fn replay_of_old_genesis_is_not_a_reset() {
    let (ac, tm, mut rx) = setup();
    let g = genesis(&ac.a, ac.id);
    tm.pin_genesis(&g).unwrap();
    let l1 = add(&g, &ac.a, &ac.b);
    tm.ingest(&l1).unwrap();
    assert_eq!(tm.ingest(&g).unwrap(), Ingest::Unchanged);
    assert!(tm.pending_resets().is_empty());
    while let Ok(e) = rx.try_recv() {
        assert!(!matches!(e, EngineEvent::TrustReset { .. }));
    }
}

#[test]
fn trust_reset_needs_confirmation_and_prompts_once_per_root() {
    let (ac, tm, mut rx) = setup();
    let g = genesis(&ac.a, ac.id);
    tm.pin_genesis(&g).unwrap();
    tm.ingest(&add(&g, &ac.a, &ac.b)).unwrap();

    let fresh = Identity::generate().unwrap();
    let new_root = genesis(&fresh, ac.id);
    let Ingest::ResetPending { root_hash } = tm.ingest(&new_root).unwrap() else {
        panic!()
    };
    // Not accepted automatically: old trust stays, new device is not trusted.
    assert!(tm.is_trusted(&ac.a.node_id()) && !tm.is_trusted(&fresh.node_id()));
    assert_eq!(seq(&tm), 1);
    // The same offer again: still pending, but no second prompt.
    assert!(matches!(
        tm.ingest(&new_root).unwrap(),
        Ingest::ResetPending { .. }
    ));
    let mut prompts = 0;
    while let Ok(e) = rx.try_recv() {
        if matches!(e, EngineEvent::TrustReset { .. }) {
            prompts += 1;
        }
    }
    assert_eq!(prompts, 1);
    assert!(tm.confirm_reset(&[9; 32]).is_err());
    tm.confirm_reset(&root_hash).unwrap();
    assert!(tm.is_trusted(&fresh.node_id()) && !tm.is_trusted(&ac.a.node_id()));
    assert_eq!(tm.root_hash(), Some(root_hash));
    // After confirming, extending the new chain works.
    let n1 = add(&new_root, &fresh, &ac.b);
    assert_eq!(tm.ingest(&n1).unwrap(), Ingest::Advanced { seq: 1 });
}

#[test]
fn pairing_chain_must_list_both_devices() {
    let (ac, tm, _rx) = setup();
    let g = genesis(&ac.a, ac.id);
    let l1 = add(&g, &ac.a, &ac.b);
    let stranger = Identity::generate().unwrap();
    let chain = [g.clone(), l1.clone()];
    assert!(
        tm.pin_paired_chain(&chain, &stranger.node_id(), &ac.a.node_id())
            .is_err()
    );
    assert!(tm.head().is_none());
    assert_eq!(
        tm.pin_paired_chain(&chain, &ac.b.node_id(), &ac.a.node_id())
            .unwrap(),
        1
    );
}

#[test]
fn file_store_persists_pin_and_blocks_rollback_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("trust.bin");
    let (ac, _, _rx) = setup();
    let g = genesis(&ac.a, ac.id);
    let l1 = add(&g, &ac.a, &ac.b);
    {
        let (sink, _r) = EventSink::channel();
        let tm = TrustManager::new(Arc::new(FileTrustStore::new(&path)), sink).unwrap();
        tm.pin_genesis(&g).unwrap();
        tm.ingest(&l1).unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let (sink, _r) = EventSink::channel();
    let tm = TrustManager::new(Arc::new(FileTrustStore::new(&path)), sink).unwrap();
    assert_eq!(seq(&tm), 1);
    assert_eq!(tm.ingest(&g).unwrap(), Ingest::Unchanged);
    assert_eq!(seq(&tm), 1);
    tm.clear().unwrap();
    assert!(tm.head().is_none() && !path.exists());
    std::fs::write(&path, b"garbage").unwrap();
    let (sink, _r) = EventSink::channel();
    assert!(TrustManager::new(Arc::new(FileTrustStore::new(&path)), sink).is_err());
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
    assert!(tm.pending_resets().is_empty());
}

#[test]
fn pending_resets_keep_only_the_newest_two() {
    let (ac, tm, mut rx) = setup();
    tm.pin_genesis(&genesis(&ac.a, ac.id)).unwrap();
    let mut roots = vec![];
    for _ in 0..5 {
        let fresh = Identity::generate().unwrap();
        let Ingest::ResetPending { root_hash } = tm.ingest(&genesis(&fresh, ac.id)).unwrap() else {
            panic!()
        };
        roots.push(root_hash);
    }
    let pending = tm.pending_resets();
    assert_eq!(pending.len(), 2);
    assert!(pending.contains(&roots[3]) && pending.contains(&roots[4]));
    assert!(tm.confirm_reset(&roots[0]).is_err());
    let mut prompts = 0;
    while let Ok(e) = rx.try_recv() {
        prompts += usize::from(matches!(e, EngineEvent::TrustReset { .. }));
    }
    assert_eq!(prompts, 5);
}
