mod common;

use common::TestApp;
use fsync_backend::auth::refresh;
use serde_json::Value;
use uuid::Uuid;

const EMAIL: &str = "alice@example.com";

/// Pretend the last rotation happened long ago (outside the 10 s retry grace window).
async fn age_rotations(app: &TestApp) {
    sqlx::query(
        "UPDATE refresh_tokens SET rotated_at = rotated_at - 60000 WHERE rotated_at IS NOT NULL",
    )
    .execute(&app.pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn refresh_rotates_and_old_reuse_revokes_family() {
    let app = TestApp::start().await;
    let s = app.login(EMAIL).await;
    let r1: Value = app.refresh(&s.refresh).await.json().await.unwrap();
    let t1 = r1["refresh"].as_str().unwrap().to_string();
    assert_ne!(t1, s.refresh);
    let ok = app
        .http
        .get(app.url("/v1/device-list"))
        .bearer_auth(r1["access"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status(), 404);

    let r2: Value = app.refresh(&t1).await.json().await.unwrap();
    let t2 = r2["refresh"].as_str().unwrap().to_string();
    age_rotations(&app).await;

    // Replaying an old (already rotated) token long after = theft: 401 and the family dies.
    assert_eq!(app.refresh(&s.refresh).await.status(), 401);
    assert_eq!(
        app.refresh(&t2).await.status(),
        401,
        "family must be revoked"
    );
    assert_eq!(app.refresh(&t1).await.status(), 401);
    assert_eq!(app.refresh("garbage").await.status(), 401);
}

#[tokio::test]
async fn concurrent_refresh_is_409_in_progress_and_family_survives() {
    let app = TestApp::start().await;
    let s = app.login(EMAIL).await;
    let (a, b) = tokio::join!(app.refresh(&s.refresh), app.refresh(&s.refresh));
    let (ok, loser) = if a.status() == 200 { (a, b) } else { (b, a) };
    assert_eq!(ok.status(), 200);
    assert_eq!(loser.status(), 409);
    assert_eq!(
        loser.json::<Value>().await.unwrap()["code"],
        "refresh_in_progress"
    );

    // The winner's new token still works: no family revocation happened.
    let t1 = ok.json::<Value>().await.unwrap()["refresh"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(app.refresh(&t1).await.status(), 200);
}

#[tokio::test]
async fn logout_revokes_session() {
    let app = TestApp::start().await;
    let s = app.login(EMAIL).await;
    let r = app
        .http
        .post(app.url("/v1/auth/logout"))
        .json(&serde_json::json!({ "refresh": s.refresh }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 204);
    assert_eq!(app.refresh(&s.refresh).await.status(), 401);
}

#[tokio::test]
async fn family_dies_90_days_after_login_and_refresh_keeps_auth_time() {
    let app = TestApp::start().await;
    let s = app.login(EMAIL).await;
    let day_ms = 24 * 3600 * 1000_i64;
    // Login 89 d ago: still refreshable, but the new token cannot outlive day 90.
    sqlx::query("UPDATE refresh_tokens SET auth_time = auth_time - $1")
        .bind(89 * day_ms)
        .execute(&app.pool)
        .await
        .unwrap();
    let r: Value = app.refresh(&s.refresh).await.json().await.unwrap();
    let (auth, exp): (i64, i64) =
        sqlx::query_as("SELECT auth_time, expires_at FROM refresh_tokens WHERE revoked_at IS NULL")
            .fetch_one(&app.pool)
            .await
            .unwrap();
    assert_eq!(
        exp,
        auth + 90 * day_ms,
        "capped by the absolute family lifetime"
    );
    // Login 91 d ago: refused.
    sqlx::query("UPDATE refresh_tokens SET auth_time = auth_time - $1")
        .bind(2 * day_ms)
        .execute(&app.pool)
        .await
        .unwrap();
    assert_eq!(
        app.refresh(r["refresh"].as_str().unwrap()).await.status(),
        401
    );
}

#[tokio::test]
async fn account_keeps_at_most_20_live_families() {
    let app = TestApp::start().await;
    let first = app.login(EMAIL).await;
    // 19 more live families (OTP start is rate limited, so create them directly).
    let mut conn = app.pool.acquire().await.unwrap();
    for _ in 0..19 {
        let now = fsync_backend::util::now_ms();
        refresh::issue(&mut conn, first.account_id, Uuid::new_v4(), now)
            .await
            .unwrap();
    }
    drop(conn);
    assert_eq!(
        app.refresh(&first.refresh).await.status(),
        200,
        "20 families: all alive"
    );
    app.login(EMAIL).await; // 21st family: the oldest goes
    let (n,): (i64,) = sqlx::query_as(
        "SELECT count(DISTINCT family_id) FROM refresh_tokens WHERE revoked_at IS NULL",
    )
    .fetch_one(&app.pool)
    .await
    .unwrap();
    assert_eq!(n, 20);
}
