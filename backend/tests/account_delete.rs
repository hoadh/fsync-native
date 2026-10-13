mod common;

use common::TestApp;
use serde_json::Value;

async fn delete(app: &TestApp, access: &str) -> reqwest::Response {
    app.http
        .delete(app.url("/v1/account"))
        .bearer_auth(access)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn delete_account_requires_fresh_auth() {
    let app = TestApp::start().await;
    let s = app.login("a@example.com").await;
    let stale = app.stale_session(&s, 10 * 60);
    let r = delete(&app, &stale.access).await;
    assert_eq!(r.status(), 401);
    assert_eq!(r.json::<Value>().await.unwrap()["code"], "reauth_required");
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM accounts")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn delete_account_cascades() {
    let app = TestApp::start().await;
    let (s, _d1, _d2) = app.setup_two_devices("a@example.com").await;
    assert!(delete(&app, &s.access).await.status().is_success());
    for table in [
        "accounts",
        "device_lists",
        "device_list_history",
        "refresh_tokens",
    ] {
        let (n,): (i64,) = sqlx::query_as(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&app.pool)
            .await
            .unwrap();
        assert_eq!(n, 0, "{table}");
    }
    assert_eq!(app.refresh(&s.refresh).await.status(), 401);
}
