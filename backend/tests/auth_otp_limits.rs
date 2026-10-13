//! OTP start/verify limits, resend rules, send failure and client-IP resolution.
mod common;

use common::TestApp;
use fsync_backend::auth::otp::OTP_TTL_MS;
use std::sync::atomic::Ordering;

const EMAIL: &str = "alice@example.com";

/// Make the pending code for `EMAIL` expired so the next start issues a new one.
async fn expire_code(app: &TestApp) {
    sqlx::query("UPDATE otp_codes SET expires_at = 1")
        .execute(&app.pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn otp_start_rate_limits_and_validates() {
    let app = TestApp::start().await;
    let r = app.start_otp("not-an-email").await;
    assert_eq!(r.status(), 400);
    for _ in 0..5 {
        assert_eq!(app.start_otp(EMAIL).await.status(), 204);
        expire_code(&app).await;
    }
    let r = app.start_otp(EMAIL).await;
    assert_eq!(r.status(), 429);
    assert!(r.headers().contains_key("retry-after"));
    // The refused start must not have touched the (expired) row's replacement path.
    assert_eq!(
        app.verify_otp(EMAIL, &app.last_code(EMAIL)).await.status(),
        401
    );
    // A different email is still fine (per-email bucket).
    assert_eq!(app.start_otp("bob@example.com").await.status(), 204);
}

#[tokio::test]
async fn noop_starts_do_not_consume_the_email_limit() {
    let app = TestApp::start().await;
    // Ten repeats against a live code are no-ops; none counts toward the 5/h email budget.
    for _ in 0..10 {
        assert_eq!(app.start_otp(EMAIL).await.status(), 204);
    }
    for _ in 0..4 {
        expire_code(&app).await;
        assert_eq!(app.start_otp(EMAIL).await.status(), 204);
    }
    // 1 + 4 = 5 code-creating starts so far: the sixth is refused.
    expire_code(&app).await;
    assert_eq!(app.start_otp(EMAIL).await.status(), 429);
}

#[tokio::test]
async fn send_failure_removes_the_code_and_allows_an_immediate_retry() {
    let app = TestApp::start().await;
    app.email.fail.store(true, Ordering::SeqCst);
    assert_eq!(app.start_otp(EMAIL).await.status(), 500);
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM otp_codes")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    assert_eq!(n, 0, "no live code the user was never told about");
    app.email.fail.store(false, Ordering::SeqCst);
    assert_eq!(app.start_otp(EMAIL).await.status(), 204);
    let code = app.last_code(EMAIL);
    assert_eq!(app.verify_otp(EMAIL, &code).await.status(), 200);
}

#[tokio::test]
async fn live_code_older_than_60s_is_replaced_by_a_new_start() {
    let app = TestApp::start().await;
    app.start_otp(EMAIL).await;
    let first = app.last_code(EMAIL);
    // Age the code to 61 s (still live: expires in ~9 min).
    let aged = fsync_backend::util::now_ms() + OTP_TTL_MS - 61_000;
    sqlx::query("UPDATE otp_codes SET expires_at = $1")
        .bind(aged)
        .execute(&app.pool)
        .await
        .unwrap();
    assert_eq!(app.start_otp(EMAIL).await.status(), 204);
    let second = app.last_code(EMAIL);
    assert_ne!(first, second, "an older live code is replaced");
    assert_eq!(app.verify_otp(EMAIL, &second).await.status(), 200);
}

#[tokio::test]
async fn repeat_start_keeps_pending_code_valid_and_still_204() {
    let app = TestApp::start().await;
    assert_eq!(app.start_otp(EMAIL).await.status(), 204);
    let code = app.last_code(EMAIL);
    // A second start (e.g. by a third party) must not invalidate the victim's code.
    assert_eq!(app.start_otp(EMAIL).await.status(), 204);
    assert_eq!(app.last_code(EMAIL), code, "no new code was generated");
    assert_eq!(app.verify_otp(EMAIL, &code).await.status(), 200);
}

#[tokio::test]
async fn burned_code_can_be_replaced_by_a_new_start() {
    let app = TestApp::start().await;
    app.start_otp(EMAIL).await;
    let good = app.last_code(EMAIL);
    let bad = if good == "000000" { "000001" } else { "000000" };
    for _ in 0..5 {
        app.verify_otp(EMAIL, bad).await;
    }
    app.start_otp(EMAIL).await;
    let code = app.last_code(EMAIL);
    assert_eq!(app.verify_otp(EMAIL, &code).await.status(), 200);
}

#[tokio::test]
async fn verify_is_rate_limited_per_ip() {
    let app = TestApp::start().await;
    for _ in 0..30 {
        assert_eq!(
            app.verify_otp("x@example.com", "123456").await.status(),
            401
        );
    }
    assert_eq!(
        app.verify_otp("x@example.com", "123456").await.status(),
        429
    );
}

#[tokio::test]
async fn forwarded_for_is_counted_from_the_right_hop() {
    let app = TestApp::start_with(|c| {
        c.trust_proxy_hops = 1;
        c.trusted_proxies = vec!["127.0.0.0/8".parse().unwrap()];
    })
    .await;
    let start = |n: usize, xff: String| {
        let app = &app;
        async move {
            app.http
                .post(app.url("/v1/auth/otp/start"))
                .header("X-Forwarded-For", xff)
                .json(&serde_json::json!({ "email": format!("u{n}@example.com") }))
                .send()
                .await
                .unwrap()
                .status()
        }
    };
    // The client prepends a different forged entry each time; the proxy-appended hop is fixed.
    for n in 0..20 {
        assert_eq!(start(n, format!("9.9.9.{n}, 203.0.113.7")).await, 204);
    }
    assert_eq!(start(20, "1.1.1.1, 203.0.113.7".into()).await, 429);
    // A different real client is unaffected.
    assert_eq!(start(21, "1.1.1.1, 203.0.113.8".into()).await, 204);
}

#[tokio::test]
async fn forwarded_for_from_an_untrusted_peer_is_ignored() {
    // The test client connects from 127.0.0.1, which is not a trusted proxy here.
    let app = TestApp::start_with(|c| {
        c.trust_proxy_hops = 1;
        c.trusted_proxies = vec!["10.0.0.0/8".parse().unwrap()];
    })
    .await;
    for n in 0..20 {
        let r = app
            .http
            .post(app.url("/v1/auth/otp/start"))
            .header("X-Forwarded-For", format!("9.9.9.{n}, 203.0.113.{n}"))
            .json(&serde_json::json!({ "email": format!("u{n}@example.com") }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 204);
    }
    // Spoofed, ever-changing XFF did not buy a fresh bucket: the peer's limit applies.
    let r = app
        .http
        .post(app.url("/v1/auth/otp/start"))
        .header("X-Forwarded-For", "8.8.8.8, 198.51.100.99")
        .json(&serde_json::json!({ "email": "late@example.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 429);
}
