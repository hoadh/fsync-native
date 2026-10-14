//! `sync_device_list` against the real backend when the account was reset to a shorter chain:
//! our incremental page is empty, only the full chain shows the new root.

mod backend_harness;
mod common;
use backend_harness::{Backend, login};
use fsync_core::{
    EventSink, Identity,
    sync::sync_device_list,
    trust::{Ingest, MemoryTrustStore, TrustManager},
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

#[tokio::test]
async fn an_empty_incremental_page_leads_to_the_reset_root_and_a_decline_sticks() {
    let be = Backend::start().await;
    let l = login(&be).await;
    let (api, account) = (l.api, l.account);
    let (a, b, c) = (
        Identity::generate().unwrap(),
        Identity::generate().unwrap(),
        Identity::generate().unwrap(),
    );
    let g = genesis(&a, account);
    api.put_device_list(&g, None).await.unwrap();
    let l1 = add(&g, &a, &b);
    api.put_device_list(&l1, Some(0)).await.unwrap();
    let l2 = add(&l1, &a, &c);
    api.put_device_list(&l2, Some(1)).await.unwrap();

    let (sink, _rx) = EventSink::channel();
    let trust = TrustManager::new(Arc::new(MemoryTrustStore::default()), sink).unwrap();
    trust.pin_genesis(&g).unwrap();
    assert_eq!(
        sync_device_list(&api, &trust).await.unwrap(),
        Ingest::Advanced { seq: 2 }
    );

    // Another device re-creates the root: a one-entry chain, shorter than our pin.
    let x = Identity::generate().unwrap();
    let root = genesis(&x, account);
    api.put_trust_reset(&root).await.unwrap();
    assert!(
        api.chain_all(2).await.unwrap().is_empty(),
        "nothing at our seq"
    );
    let want = Ingest::ResetPending {
        head_hash: root.hash().unwrap(),
    };
    assert_eq!(sync_device_list(&api, &trust).await.unwrap(), want);
    assert_eq!(
        sync_device_list(&api, &trust).await.unwrap(),
        want,
        "stays pending"
    );
    assert_eq!(
        trust.list().unwrap().seq,
        2,
        "nothing pinned without a confirm"
    );

    // Declined: later syncs, also after the chain grows, report no change and pin nothing.
    assert!(trust.decline_reset(&root.hash().unwrap()).unwrap());
    assert_eq!(
        sync_device_list(&api, &trust).await.unwrap(),
        Ingest::Unchanged
    );
    let y = Identity::generate().unwrap();
    let grown = add(&root, &x, &y);
    api.put_device_list(&grown, Some(0)).await.unwrap();
    assert_eq!(
        sync_device_list(&api, &trust).await.unwrap(),
        Ingest::Unchanged
    );
    assert!(trust.is_declined_head(&grown.hash().unwrap()));
    assert_eq!(
        sync_device_list(&api, &trust).await.unwrap(),
        Ingest::Unchanged
    );
    assert!(trust.pending_reset_roots().is_empty());
    assert_eq!(trust.list().unwrap().seq, 2);
}
