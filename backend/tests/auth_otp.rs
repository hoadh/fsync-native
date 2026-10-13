mod common;

use common::TestApp;
use serde_json::Value;

const EMAIL: &str = "alice@example.com";

#[tokio::test]
async fn otp_flow_and_authenticated_access() {
    let app = TestApp::start().await;
    // No token -> 401; health ok.
    assert_eq!(
        app.http
            .get(app.url("/health"))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let r = app
        .http
        .get(app.url("/v1/device-list"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);

    let s = app.login("alice@example.com").await;
    let r = app
        .http
        .get(app.url("/v1/device-list"))
        .bearer_auth(&s.access)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404); // authenticated, no list yet
    let body: Value = r.json().await.unwrap();
    assert_eq!(body["code"], "not_found");

    // Same email logs into the same account; mixed case normalizes.
    app.start_otp("  Alice@Example.com ").await;
    let code = app.last_code("alice@example.com");
    let again: Value = app
        .verify_otp("ALICE@example.com", &code)
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(again["account_id"], s.account_id.to_string());

    // Code is single use.
    assert_eq!(app.verify_otp(EMAIL, &code).await.status(), 401);
}

#[tokio::test]
async fn wrong_code_burns_attempts_then_locks_out() {
    let app = TestApp::start().await;
    app.start_otp(EMAIL).await;
    let good = app.last_code(EMAIL);
    let bad = if good == "000000" { "000001" } else { "000000" };
    for _ in 0..5 {
        assert_eq!(app.verify_otp(EMAIL, bad).await.status(), 401);
    }
    // Even the right code fails after 5 attempts.
    assert_eq!(app.verify_otp(EMAIL, &good).await.status(), 401);
    // Malformed code and unknown email also 401 (no oracle).
    assert_eq!(app.verify_otp(EMAIL, "12ab56").await.status(), 401);
    assert_eq!(
        app.verify_otp("nobody@example.com", "123456")
            .await
            .status(),
        401
    );
    // A fresh start resets attempts.
    app.start_otp(EMAIL).await;
    let code = app.last_code(EMAIL);
    assert_eq!(app.verify_otp(EMAIL, &code).await.status(), 200);
}

#[tokio::test]
async fn expired_otp_is_rejected() {
    let app = TestApp::start().await;
    app.start_otp(EMAIL).await;
    let code = app.last_code(EMAIL);
    sqlx::query("UPDATE otp_codes SET expires_at = 1")
        .execute(&app.pool)
        .await
        .unwrap();
    assert_eq!(app.verify_otp(EMAIL, &code).await.status(), 401);
}
