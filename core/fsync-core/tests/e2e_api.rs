//! api_client against the REAL backend: device list, conflicts, chain, pairing, token refresh.

mod backend_harness;
mod common;
use backend_harness::{Backend, api_code, login};
use common::peer;
use fsync_core::CoreError;
use fsync_proto::{DeviceList, Platform, sign};

#[tokio::test]
async fn device_list_api_conflicts_chain_and_token_refresh() {
    let be = Backend::start().await;
    let l = login(&be).await;
    let a = peer().await;
    assert!(l.api.get_device_list().await.unwrap().is_none());

    let g = a.genesis(l.account);
    let put = l.api.put_device_list(&g, None).await.unwrap();
    assert_eq!(put.decode().unwrap().seq, 0);
    assert_eq!(
        l.api.get_device_list().await.unwrap().unwrap().hash(),
        g.hash()
    );

    let b = peer().await;
    let n1 = sign(
        a.id.signing_key(),
        &DeviceList::next_add(&g, a.nid(), b.nid(), "b".into(), Platform::Mac, 2).unwrap(),
    )
    .unwrap();
    // Stale If-Match -> 409; correct one succeeds.
    let (status, _) = api_code(l.api.put_device_list(&n1, Some(7)).await.unwrap_err());
    assert_eq!(status, 409);
    l.api.put_device_list(&n1, Some(0)).await.unwrap();
    let chain = l.api.chain_all(0).await.unwrap();
    assert_eq!(
        chain
            .iter()
            .map(|s| s.decode().unwrap().seq)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(l.api.chain_all(1).await.unwrap().len(), 1);

    // Pairing: offer + lookup; bad code format rejected locally; unknown code -> 404.
    let created = l.api.create_pairing(b.nid(), None).await.unwrap();
    let found = l.api.lookup_pairing(&created.code).await.unwrap();
    assert_eq!(found.node_id, b.nid());
    assert!(matches!(
        l.api.lookup_pairing("12ab").await,
        Err(CoreError::Invalid(_))
    ));
    assert_eq!(
        api_code(l.api.lookup_pairing("000000").await.unwrap_err()).0,
        404
    );

    // Refresh rotates tokens and reports them to the app callback.
    let before = l.api.tokens().unwrap().refresh;
    l.api.refresh_now().await.unwrap();
    let after = l.api.tokens().unwrap().refresh;
    assert_ne!(before, after);
    assert_eq!(l.refreshed.lock().unwrap().last(), Some(&after));
    assert!(l.api.get_device_list().await.unwrap().is_some());
}

#[tokio::test]
async fn unauthenticated_client_gets_auth_expired_after_logout() {
    let be = Backend::start().await;
    let l = login(&be).await;
    let refresh = l.api.take_refresh().unwrap();
    l.api.revoke(&refresh).await.unwrap();
    assert!(l.api.tokens().is_none());
    assert!(matches!(
        l.api.get_device_list().await,
        Err(CoreError::AuthExpired)
    ));
}
