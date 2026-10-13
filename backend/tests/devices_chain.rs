mod common;

use common::{
    TestApp,
    devices::{Device, add, genesis},
};
use fsync_proto::{
    DeviceList, Platform, SignedDeviceList,
    api_dto::{ChainResponse, DeviceListResponse, decode_signed},
    sign,
};
use serde_json::Value;

async fn get_list(app: &TestApp, access: &str) -> reqwest::Response {
    app.http
        .get(app.url("/v1/device-list"))
        .bearer_auth(access)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn genesis_next_and_conflict() {
    let app = TestApp::start().await;
    let s = app.login("a@example.com").await;
    let (d1, d2) = (Device::new(), Device::new());
    let g = genesis(&d1, s.account_id);

    // Genesis with a stale If-Match is a conflict; absent / "none" succeeds.
    assert_eq!(app.put_list(&s, &g, Some("0"), false).await.status(), 409);
    assert_eq!(app.put_list(&s, &g, None, false).await.status(), 200);
    // Idempotent re-PUT of identical bytes.
    assert_eq!(app.put_list(&s, &g, Some("0"), false).await.status(), 200);

    let got: DeviceListResponse = get_list(&app, &s.access).await.json().await.unwrap();
    assert_eq!(got.seq, 0);
    assert_eq!(got.decode().unwrap(), g);

    let l1 = add(&g, &d1, &d2);
    // Wrong If-Match -> 409; missing -> 409 (list exists).
    assert_eq!(app.put_list(&s, &l1, Some("5"), false).await.status(), 409);
    assert_eq!(app.put_list(&s, &l1, None, false).await.status(), 409);
    let ok = app.put_list(&s, &l1, Some("0"), false).await;
    assert_eq!(ok.status(), 200);
    assert_eq!(ok.json::<DeviceListResponse>().await.unwrap().seq, 1);

    // Retry of the already-stored list is idempotent even with a stale If-Match...
    assert_eq!(app.put_list(&s, &l1, Some("0"), false).await.status(), 200);
    // ...but a different stale writer (If-Match 0 again) loses.
    let other = add(&g, &d1, &Device::new());
    assert_eq!(
        app.put_list(&s, &other, Some("0"), false).await.status(),
        409
    );
}

#[tokio::test]
async fn forged_gap_rollback_and_foreign_account_are_422() {
    let app = TestApp::start().await;
    let s = app.login("a@example.com").await;
    let (d1, d2, attacker) = (Device::new(), Device::new(), Device::new());
    let g = genesis(&d1, s.account_id);
    app.put_list(&s, &g, None, false).await;
    let l1 = add(&g, &d1, &d2);
    app.put_list(&s, &l1, Some("0"), false).await;

    // Signed by a key that is not in the list.
    let forged = add(&l1, &attacker, &attacker);
    let r = app.put_list(&s, &forged, Some("1"), false).await;
    assert_eq!(r.status(), 422);
    assert_eq!(r.json::<Value>().await.unwrap()["code"], "invalid_chain");

    // Gap (seq 3 on top of 1).
    let l2 = add(&l1, &d1, &Device::new());
    let l3 = add(&l2, &d1, &Device::new());
    assert_eq!(app.put_list(&s, &l3, Some("1"), false).await.status(), 422);

    // Rollback: a different seq-1 list on top of seq 1.
    let alt = DeviceList::next_add(
        &g,
        d1.node,
        Device::new().node,
        "x".into(),
        Platform::Cli,
        9,
    )
    .unwrap();
    let alt = sign(&d1.key, &alt).unwrap();
    assert_eq!(app.put_list(&s, &alt, Some("1"), false).await.status(), 422);

    // Genesis for another account's id.
    let s2 = app.login("b@example.com").await;
    let foreign = genesis(&Device::new(), s.account_id);
    assert_eq!(app.put_list(&s2, &foreign, None, false).await.status(), 422);

    // Undecodable body -> 400.
    let r = app
        .http
        .put(app.url("/v1/device-list"))
        .bearer_auth(&s.access)
        .header("If-Match", "1")
        .json(&serde_json::json!({ "signed": "!!!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);

    // Stored list is unchanged.
    let got: DeviceListResponse = get_list(&app, &s.access).await.json().await.unwrap();
    assert_eq!(got.seq, 1);
}

#[tokio::test]
async fn chain_fetch_returns_consecutive_range() {
    let app = TestApp::start().await;
    let s = app.login("a@example.com").await;
    let d1 = Device::new();
    let mut lists: Vec<SignedDeviceList> = vec![genesis(&d1, s.account_id)];
    app.put_list(&s, &lists[0], None, false).await;
    for i in 0..3 {
        let next = add(lists.last().unwrap(), &d1, &Device::new());
        let r = app.put_list(&s, &next, Some(&i.to_string()), false).await;
        assert_eq!(r.status(), 200);
        lists.push(next);
    }
    let fetch = |from: &str| {
        let url = app.url(&format!("/v1/device-list/chain{from}"));
        let req = app.http.get(url).bearer_auth(&s.access);
        async move {
            req.send()
                .await
                .unwrap()
                .json::<ChainResponse>()
                .await
                .unwrap()
        }
    };
    let all = fetch("").await;
    assert_eq!(all.chain.len(), 4);
    let from2 = fetch("?from_seq=2").await;
    let decoded: Vec<_> = from2
        .chain
        .iter()
        .map(|c| decode_signed(c).unwrap())
        .collect();
    assert_eq!(decoded, lists[2..].to_vec());
    // The chain verifies on top of the pinned seq-1 list.
    let out = fsync_proto::verify_chain(&lists[1], &decoded).unwrap();
    assert!(matches!(out, fsync_proto::Outcome::Advanced(l) if l.seq == 3));
}

#[tokio::test]
async fn chain_reports_earliest_seq_and_truncation() {
    let app = TestApp::start().await;
    let s = app.login("a@example.com").await;
    let d1 = Device::new();
    let g = genesis(&d1, s.account_id);
    app.put_list(&s, &g, None, false).await;
    let next = add(&g, &d1, &Device::new());
    app.put_list(&s, &next, Some("0"), false).await;
    let fetch = |from: i64| {
        let req = app
            .http
            .get(app.url(&format!("/v1/device-list/chain?from_seq={from}")))
            .bearer_auth(&s.access);
        async move {
            req.send()
                .await
                .unwrap()
                .json::<ChainResponse>()
                .await
                .unwrap()
        }
    };
    let r = fetch(0).await;
    assert_eq!((r.chain.len(), r.earliest_seq, r.truncated), (2, 0, false));

    // Pretend 150 more retained rows exist: the first page is full and flagged truncated.
    sqlx::query(
        "INSERT INTO device_list_history (account_id, epoch, seq, signed, created_at)
         SELECT account_id, epoch, 1000 + g, signed, created_at
         FROM device_list_history, generate_series(0, 149) g WHERE seq = 0",
    )
    .execute(&app.pool)
    .await
    .unwrap();
    let r = fetch(1000).await;
    assert_eq!(r.chain.len(), 100);
    assert!(r.truncated);
    let r = fetch(1100).await;
    assert_eq!((r.chain.len(), r.truncated), (50, false));
    // Pruned history is visible: earliest_seq moves up.
    sqlx::query("DELETE FROM device_list_history WHERE seq < 1")
        .execute(&app.pool)
        .await
        .unwrap();
    assert_eq!(fetch(0).await.earliest_seq, 1);
}
