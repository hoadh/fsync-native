//! A device whose history was reset (trust gone) is still in the account's list under its node
//! id. Pairing it again must succeed without publishing a new list.

mod common;
use common::{
    Peer,
    pairing::{pair, pair_on},
    peer,
};
use fsync_core::{
    EventSink, PairSecret,
    net::Node,
    trust::{MemoryTrustStore, TrustManager},
};
use std::sync::Arc;
use uuid::Uuid;
use zeroize::Zeroizing;

fn code(c: &str) -> PairSecret {
    PairSecret::Code(Zeroizing::new(c.to_owned()))
}

/// Same identity, empty trust: what a device looks like after `reset_history`.
async fn after_reset(old: Peer) -> Peer {
    let id = old.id.clone();
    drop(old);
    let (sink, events) = EventSink::channel();
    let trust =
        Arc::new(TrustManager::new(Arc::new(MemoryTrustStore::default()), sink.clone()).unwrap());
    let node = Node::builder(id.clone(), trust.clone(), sink)
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

#[tokio::test]
async fn a_reset_device_can_pair_again_without_a_new_list() {
    let (mut a, b) = (peer().await, peer().await);
    let mut b = b;
    let (out, backend) = pair(&mut a, &mut b, code("123456"), code("123456"), true).await;
    assert_eq!(*out.existing.as_ref().unwrap(), 1);
    let account = a.trust.list().unwrap().account_id;

    let mut b = after_reset(b).await;
    assert!(b.trust.list().is_none(), "trust is gone after the reset");
    let out = pair_on(
        &mut a,
        &mut b,
        code("654321"),
        code("654321"),
        true,
        true,
        account,
        &backend,
    )
    .await;
    assert_eq!(*out.existing.as_ref().unwrap(), 1, "head seq unchanged");
    assert_eq!(*out.new.as_ref().unwrap(), 1);
    assert!(out.sas_existing.is_some() && out.sas_existing == out.sas_new);
    assert_eq!(backend.0.lock().unwrap().len(), 2, "nothing new published");
    assert!(b.trust.is_trusted(&a.nid()) && b.trust.is_trusted(&b.nid()));
    assert_eq!(a.trust.list().unwrap().seq, 1);
}

#[tokio::test]
async fn re_pairing_still_needs_both_sides_to_accept() {
    let (mut a, mut b) = (peer().await, peer().await);
    let (_, backend) = pair(&mut a, &mut b, code("123456"), code("123456"), true).await;
    let account: Uuid = a.trust.list().unwrap().account_id;
    let mut b = after_reset(b).await;
    let out = pair_on(
        &mut a,
        &mut b,
        code("654321"),
        code("654321"),
        true,
        false,
        account,
        &backend,
    )
    .await;
    assert!(out.existing.is_err() && out.new.is_err());
    assert!(b.trust.list().is_none(), "nothing pinned without consent");
}
