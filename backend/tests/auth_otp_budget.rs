//! Global rolling-24h OTP send cap, and refunds of the per-email / per-IP quota on a failed send.
mod common;

use common::TestApp;
use std::sync::atomic::Ordering;

const EMAIL: &str = "alice@example.com";

async fn sends_logged(app: &TestApp) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM otp_send_log")
        .fetch_one(&app.pool)
        .await
        .unwrap()
}

async fn expire_codes(app: &TestApp) {
    sqlx::query("UPDATE otp_codes SET expires_at = 1")
        .execute(&app.pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn global_cap_refuses_with_429_without_sending() {
    let app = TestApp::start_with(|c| c.otp_daily_send_cap = 2).await;
    assert_eq!(app.start_otp("a@example.com").await.status(), 204);
    assert_eq!(app.start_otp("b@example.com").await.status(), 204);
    let r = app.start_otp("c@example.com").await;
    assert_eq!(r.status(), 429);
    let wait: u64 = r.headers()["retry-after"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=86_400).contains(&wait), "retry-after {wait}");
    assert!(
        app.email
            .codes
            .lock()
            .unwrap()
            .get("c@example.com")
            .is_none(),
        "nothing sent"
    );
    let (n,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM otp_codes WHERE email = 'c@example.com'")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(n, 0, "no live code left behind");
}

#[tokio::test]
async fn sends_older_than_24h_free_their_slots() {
    let app = TestApp::start_with(|c| c.otp_daily_send_cap = 1).await;
    assert_eq!(app.start_otp("a@example.com").await.status(), 204);
    assert_eq!(app.start_otp("b@example.com").await.status(), 429);
    sqlx::query("UPDATE otp_send_log SET sent_at = sent_at - 86400001")
        .execute(&app.pool)
        .await
        .unwrap();
    assert_eq!(app.start_otp("b@example.com").await.status(), 204);
    assert_eq!(sends_logged(&app).await, 1, "expired rows are pruned");
}

#[tokio::test]
async fn failed_send_refunds_global_email_and_ip_quota() {
    let app = TestApp::start_with(|c| c.otp_daily_send_cap = 1).await;
    app.email.fail.store(true, Ordering::SeqCst);
    // Far more failures than the per-email (5) and per-IP (20) budgets, and than the global cap.
    for _ in 0..25 {
        assert_eq!(app.start_otp(EMAIL).await.status(), 500);
    }
    assert_eq!(
        sends_logged(&app).await,
        0,
        "failed sends spend no global slot"
    );
    app.email.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        app.start_otp(EMAIL).await.status(),
        204,
        "no lockout after the outage"
    );
    expire_codes(&app).await;
}

#[tokio::test]
async fn cap_refusal_does_not_burn_the_ip_quota() {
    let app = TestApp::start_with(|c| c.otp_daily_send_cap = 0).await;
    for _ in 0..25 {
        assert_eq!(app.start_otp(EMAIL).await.status(), 429);
    }
}
