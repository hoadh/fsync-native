mod common;

use common::{
    TestApp,
    devices::{Device, add, genesis},
    ws::recv,
};
use fsync_proto::api_dto::ChainResponse;
use fsync_proto::{api_dto::DeviceListResponse, presence::ServerMsg};
use serde_json::Value;

#[tokio::test]
async fn trust_reset_requires_header_and_replaces_chain() {
    let app = TestApp::start().await;
    let s = app.login("a@example.com").await;
    let (d1, d2) = (Device::new(), Device::new());
    let g = genesis(&d1, s.account_id);
    app.put_list(&s, &g, None, false).await;
    app.put_list(&s, &add(&g, &d1, &d2), Some("0"), false).await;

    // New root from a brand-new device.
    let fresh = Device::new();
    let g2 = genesis(&fresh, s.account_id);
    let r = app.put_list(&s, &g2, Some("1"), false).await;
    assert_eq!(r.status(), 422);
    assert_eq!(
        r.json::<Value>().await.unwrap()["code"],
        "trust_reset_required"
    );

    // With the header it replaces the chain; chain endpoint serves only the new epoch.
    assert_eq!(app.put_list(&s, &g2, None, true).await.status(), 200);
    let got: DeviceListResponse = app
        .http
        .get(app.url("/v1/device-list"))
        .bearer_auth(&s.access)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!((got.seq, got.decode().unwrap()), (0, g2.clone()));
    let chain: ChainResponse = app
        .http
        .get(app.url("/v1/device-list/chain"))
        .bearer_auth(&s.access)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(chain.chain.len(), 1);
    // Older-epoch history is pruned at the reset: only the new root remains.
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM device_list_history")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    // The new chain continues normally.
    let next = add(&g2, &fresh, &Device::new());
    assert_eq!(
        app.put_list(&s, &next, Some("0"), false).await.status(),
        200
    );
    // Header does not bypass verification: a non-genesis list is still rejected.
    assert_eq!(
        app.put_list(&s, &add(&next, &fresh, &Device::new()), None, true)
            .await
            .status(),
        422
    );
}

#[tokio::test]
async fn trust_reset_without_fresh_auth_is_401_reauth_required() {
    let app = TestApp::start().await;
    let (s, d1, _d2) = app.setup_two_devices("a@example.com").await;
    let stale = app.stale_session(&s, 10 * 60);
    let g2 = genesis(&Device::new(), s.account_id);

    let r = app.put_list(&stale, &g2, None, true).await;
    assert_eq!(r.status(), 401);
    assert_eq!(r.json::<Value>().await.unwrap()["code"], "reauth_required");
    // Stored list untouched; ordinary PUTs and reads still work with the stale session.
    assert_eq!(app.current_list(&s).await.decode().unwrap().seq, 1);
    let next = add(&app.current_list(&s).await, &d1, &Device::new());
    assert_eq!(
        app.put_list(&stale, &next, Some("1"), false).await.status(),
        200
    );
    // Inside the window it is accepted.
    let recent = app.stale_session(&s, 60);
    assert_eq!(app.put_list(&recent, &g2, None, true).await.status(), 200);
}

#[tokio::test]
async fn trust_reset_is_limited_to_3_per_day_and_broadcast() {
    let app = TestApp::start().await;
    let (s, d1, d2) = app.setup_two_devices("a@example.com").await;
    let mut w1 = app.ws_auth(&s, &d1).await;
    let _ = recv(&mut w1).await; // Peers
    let mut w2 = app.ws_auth(&s, &d2).await;
    let _ = recv(&mut w2).await; // Peers
    let _ = recv(&mut w1).await; // PeerUpdate: d2 online

    // Reset to a genesis that keeps only d1: d1 is told, d2 (removed) is disconnected.
    let g = genesis(&d1, s.account_id);
    assert_eq!(app.put_list(&s, &g, None, true).await.status(), 200);
    assert_eq!(
        recv(&mut w1).await,
        Some(ServerMsg::DeviceListChanged { seq: 0 })
    );
    assert!(
        recv(&mut w2).await.is_none(),
        "removed device must be closed"
    );

    // Re-sending the same genesis is an idempotent no-op and does not use the budget.
    for _ in 0..3 {
        assert_eq!(app.put_list(&s, &g, None, true).await.status(), 200);
    }
    // 3 per day per account: two more distinct resets fit (1 used above), the next is refused.
    for _ in 0..2 {
        let g = genesis(&Device::new(), s.account_id);
        assert_eq!(app.put_list(&s, &g, None, true).await.status(), 200);
    }
    let g = genesis(&Device::new(), s.account_id);
    assert_eq!(app.put_list(&s, &g, None, true).await.status(), 429);
}

#[tokio::test]
async fn failed_resets_do_not_consume_the_daily_budget() {
    let app = TestApp::start().await;
    let (s, d1, _d2) = app.setup_two_devices("a@example.com").await;
    // A non-genesis list with the reset header is rejected (422) every time...
    let bogus = add(&app.current_list(&s).await, &d1, &Device::new());
    for _ in 0..5 {
        assert_eq!(app.put_list(&s, &bogus, None, true).await.status(), 422);
    }
    // ...and a stale session (401) does not count either...
    let stale = app.stale_session(&s, 10 * 60);
    let g = genesis(&Device::new(), s.account_id);
    assert_eq!(app.put_list(&stale, &g, None, true).await.status(), 401);
    // ...so all three real resets still go through.
    for _ in 0..3 {
        let g = genesis(&Device::new(), s.account_id);
        assert_eq!(app.put_list(&s, &g, None, true).await.status(), 200);
    }
    let g = genesis(&Device::new(), s.account_id);
    assert_eq!(app.put_list(&s, &g, None, true).await.status(), 429);
}
