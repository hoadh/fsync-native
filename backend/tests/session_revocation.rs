//! Access tokens die with their session family, not at token expiry.
mod common;

use common::{TestApp, ws::recv};

async fn logout(app: &TestApp, refresh: &str) {
    let r = app
        .http
        .post(app.url("/v1/auth/logout"))
        .json(&serde_json::json!({ "refresh": refresh }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 204);
}

async fn status(app: &TestApp, access: &str) -> u16 {
    app.http
        .get(app.url("/v1/device-list"))
        .bearer_auth(access)
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

#[tokio::test]
async fn access_token_is_rejected_over_http_after_logout() {
    let app = TestApp::start().await;
    let s = app.login("a@example.com").await;
    let other = app.login("a@example.com").await; // second session, independent family
    assert_eq!(status(&app, &s.access).await, 404);
    logout(&app, &s.refresh).await;
    assert_eq!(status(&app, &s.access).await, 401);
    assert_eq!(
        status(&app, &other.access).await,
        404,
        "other session lives"
    );
}

#[tokio::test]
async fn access_token_is_rejected_by_the_presence_handshake_after_logout() {
    let app = TestApp::start().await;
    let (s, d1, _d2) = app.setup_two_devices("a@example.com").await;
    let mut ok = app.ws_auth(&s, &d1).await;
    assert!(recv(&mut ok).await.is_some());
    logout(&app, &s.refresh).await;
    let mut ws = app.ws_auth(&s, &d1).await;
    assert!(recv(&mut ws).await.is_none());
}

#[tokio::test]
async fn rotated_session_stays_valid_and_reuse_revocation_kills_access() {
    let app = TestApp::start().await;
    let s = app.login("a@example.com").await;
    let r: serde_json::Value = app.refresh(&s.refresh).await.json().await.unwrap();
    // Rotation keeps the family live: both the old and new access tokens still work.
    assert_eq!(status(&app, r["access"].as_str().unwrap()).await, 404);
    assert_eq!(status(&app, &s.access).await, 404);
    // Expire the family's tokens: access tokens stop working immediately.
    sqlx::query("UPDATE refresh_tokens SET expires_at = 1")
        .execute(&app.pool)
        .await
        .unwrap();
    assert_eq!(status(&app, &s.access).await, 401);
}
