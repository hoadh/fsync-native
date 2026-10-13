mod common;

use common::{
    TestApp,
    devices::{Device, add, genesis},
};
use fsync_backend::devices::repo;
use fsync_proto::{DeviceList, Platform, sign};
use serde_json::Value;

#[tokio::test]
async fn concurrent_puts_with_same_if_match_one_wins() {
    let app = TestApp::start().await;
    let s = app.login("a@example.com").await;
    let d1 = Device::new();
    let g = genesis(&d1, s.account_id);
    assert_eq!(app.put_list(&s, &g, None, false).await.status(), 200);

    // Two different valid successors, both claiming to build on seq 0.
    let (a, b) = (add(&g, &d1, &Device::new()), add(&g, &d1, &Device::new()));
    let (ra, rb) = tokio::join!(
        app.put_list(&s, &a, Some("0"), false),
        app.put_list(&s, &b, Some("0"), false)
    );
    let mut codes = [ra.status().as_u16(), rb.status().as_u16()];
    codes.sort();
    assert_eq!(codes, [200, 409]);
    let winner = if ra.status() == 200 { &a } else { &b };
    assert_eq!(&app.current_list(&s).await, winner);
}

#[tokio::test]
async fn concurrent_first_puts_one_wins() {
    let app = TestApp::start().await;
    let s = app.login("a@example.com").await;
    let (a, b) = (
        genesis(&Device::new(), s.account_id),
        genesis(&Device::new(), s.account_id),
    );
    let (ra, rb) = tokio::join!(
        app.put_list(&s, &a, None, false),
        app.put_list(&s, &b, None, false)
    );
    let mut codes = [ra.status().as_u16(), rb.status().as_u16()];
    codes.sort();
    assert_eq!(codes, [200, 409]);
}

#[tokio::test]
async fn device_list_shape_caps_are_enforced_by_the_server() {
    let app = TestApp::start().await;
    let s = app.login("a@example.com").await;
    let d1 = Device::new();
    for bad in ["x\u{202E}evil", "line\nbreak", "", &"n".repeat(65)] {
        let l = DeviceList::genesis(s.account_id, d1.node, bad.into(), Platform::Mac, 1);
        let r = app
            .put_list(&s, &sign(&d1.key, &l).unwrap(), None, false)
            .await;
        assert_eq!(r.status(), 422, "{bad:?}");
        assert_eq!(r.json::<Value>().await.unwrap()["code"], "invalid_chain");
    }
}

#[tokio::test]
async fn puts_are_rate_limited_per_account() {
    let app = TestApp::start().await;
    let s = app.login("a@example.com").await;
    let g = genesis(&Device::new(), s.account_id);
    for _ in 0..30 {
        assert_eq!(app.put_list(&s, &g, None, false).await.status(), 200);
    }
    let r = app.put_list(&s, &g, None, false).await;
    assert_eq!(r.status(), 429);
    assert!(r.headers().contains_key("retry-after"));
}

#[tokio::test]
async fn history_is_pruned_to_last_100_per_epoch() {
    let app = TestApp::start().await;
    let s = app.login("a@example.com").await;
    let g = genesis(&Device::new(), s.account_id);
    let mut conn = app.pool.acquire().await.unwrap();
    for seq in 0..130_i64 {
        assert!(
            repo::store(&mut conn, s.account_id, 0, seq, &g)
                .await
                .unwrap()
        );
    }
    let epoch0 = || async {
        sqlx::query_as::<_, (i32, i64)>(
            "SELECT epoch, seq FROM device_list_history WHERE epoch = 0 ORDER BY seq",
        )
        .fetch_all(&app.pool)
        .await
        .unwrap()
    };
    let rows = epoch0().await;
    assert_eq!(rows.len(), 100);
    assert_eq!(rows.first().unwrap().1, 30);
    assert_eq!(rows.last().unwrap().1, 129);
    // A later epoch replaces the older epoch's history entirely.
    assert!(
        repo::store(&mut conn, s.account_id, 1, 0, &g)
            .await
            .unwrap()
    );
    assert!(epoch0().await.is_empty());
    let (n1,): (i64,) = sqlx::query_as("SELECT count(*) FROM device_list_history WHERE epoch = 1")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n1, 1);
}
