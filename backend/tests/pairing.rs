mod common;

use common::{TestApp, devices::Device};
use fsync_proto::api_dto::{PairingCreated, PairingLookup};
use serde_json::{Value, json};

async fn create(app: &TestApp, access: &str, dev: &Device) -> reqwest::Response {
    app.http
        .post(app.url("/v1/pairing"))
        .bearer_auth(access)
        .json(&json!({ "node_id": dev.node, "relay_url": "https://relay.example.com" }))
        .send()
        .await
        .unwrap()
}

async fn lookup(app: &TestApp, access: &str, code: &str) -> reqwest::Response {
    app.http
        .get(app.url(&format!("/v1/pairing/{code}")))
        .bearer_auth(access)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn create_lookup_is_single_use() {
    let app = TestApp::start().await;
    let s = app.login("a@example.com").await;
    let dev = Device::new();
    let created: PairingCreated = create(&app, &s.access, &dev).await.json().await.unwrap();
    assert_eq!(created.code.len(), 6);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    assert!(created.expires_at > now && created.expires_at <= now + 5 * 60 * 1000 + 1000);

    // Code is not stored in clear.
    let (n,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM pairing_codes WHERE encode(code_hash,'escape') LIKE $1",
    )
    .bind(format!("%{}%", created.code))
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(n, 0);

    let r = lookup(&app, &s.access, &created.code).await;
    assert_eq!(r.status(), 200);
    let found: PairingLookup = r.json().await.unwrap();
    assert_eq!(found.node_id, dev.node);
    assert_eq!(
        found.relay_url.as_deref(),
        Some("https://relay.example.com")
    );

    assert_eq!(lookup(&app, &s.access, &created.code).await.status(), 404);
}

#[tokio::test]
async fn expired_code_not_found() {
    let app = TestApp::start().await;
    let s = app.login("a@example.com").await;
    let created: PairingCreated = create(&app, &s.access, &Device::new())
        .await
        .json()
        .await
        .unwrap();
    sqlx::query("UPDATE pairing_codes SET expires_at = 1")
        .execute(&app.pool)
        .await
        .unwrap();
    assert_eq!(lookup(&app, &s.access, &created.code).await.status(), 404);
}

#[tokio::test]
async fn other_account_cannot_look_up() {
    let app = TestApp::start().await;
    let (a, b) = (
        app.login("a@example.com").await,
        app.login("b@example.com").await,
    );
    let created: PairingCreated = create(&app, &a.access, &Device::new())
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(lookup(&app, &b.access, &created.code).await.status(), 404);
    // The owner's code was not consumed by the failed cross-account attempt.
    assert_eq!(lookup(&app, &a.access, &created.code).await.status(), 200);
}

#[tokio::test]
async fn validation_auth_and_rate_limit() {
    let app = TestApp::start().await;
    let s = app.login("a@example.com").await;
    assert_eq!(lookup(&app, &s.access, "12345").await.status(), 400);
    assert_eq!(lookup(&app, &s.access, "abcdef").await.status(), 400);
    let r = app
        .http
        .post(app.url("/v1/pairing"))
        .bearer_auth(&s.access)
        .json(&json!({ "node_id": Device::new().node, "relay_url": "javascript:alert(1)" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);
    let r = app
        .http
        .get(app.url("/v1/pairing/123456"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);

    // 30 lookups/hour/account total (2 already used above before validation? they count).
    let mut limited = false;
    for _ in 0..40 {
        let r = lookup(&app, &s.access, "999999").await;
        if r.status() == 429 {
            limited = true;
            let b: Value = r.json().await.unwrap();
            assert_eq!(b["code"], "rate_limited");
            break;
        }
    }
    assert!(limited);
}
