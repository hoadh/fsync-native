//! `revoke_session` (engine-free hard logout) and `logout_confirmed` against the real backend:
//! revokes the whole session, treats 401 as done, and stays bounded and retryable on a bad
//! network (a TCP forwarder that is cut or black-holed).

mod backend_harness;
mod common;
mod service_harness;
use backend_harness::{
    Backend, login,
    proxy::{Mode, Proxy},
};
use fsync_core::service::{FsyncError, REVOKE_SESSION_BUDGET, revoke_session};
use service_harness::dev_with;
use std::time::{Duration, Instant};

/// Service calls refuse to run on a runtime thread: use a plain one.
async fn off_rt<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.await.expect("thread panicked")
}

async fn status(base: &str, access: &str) -> u16 {
    reqwest::Client::new()
        .get(format!("{base}/v1/device-list"))
        .bearer_auth(access)
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

async fn revoke(base: &str, refresh: &str, budget: Duration) -> Result<(), FsyncError> {
    let (base, refresh) = (base.to_owned(), refresh.to_owned());
    off_rt(move || revoke_session(&base, &refresh, true, budget)).await
}

#[tokio::test(flavor = "multi_thread")]
async fn revokes_the_session_and_a_second_call_counts_as_done() {
    let be = Backend::start().await;
    let t = login(&be).await.api.tokens().unwrap();
    assert_ne!(status(&be.base, &t.access).await, 401, "session is live");

    revoke(&be.base, &t.refresh, REVOKE_SESSION_BUDGET)
        .await
        .expect("revoked");
    assert_eq!(status(&be.base, &t.access).await, 401, "session is dead");
    // The pending-revoke retry may run twice (crash between success and delete): 401 is success.
    revoke(&be.base, &t.refresh, REVOKE_SESSION_BUDGET)
        .await
        .expect("already revoked is success");
    revoke(&be.base, "not-a-real-refresh-token", REVOKE_SESSION_BUDGET)
        .await
        .expect("unknown token is a 401, hence success");
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_bad_input_without_touching_the_network() {
    let be = Backend::start().await;
    let r =
        off_rt(|| revoke_session("http://127.0.0.1:1", "tok", false, REVOKE_SESSION_BUDGET)).await;
    assert!(
        matches!(r, Err(FsyncError::Invalid(_))),
        "http needs dev mode: {r:?}"
    );
    let r = revoke(&be.base, "", REVOKE_SESSION_BUDGET).await;
    assert!(matches!(r, Err(FsyncError::Invalid(_))), "{r:?}");
    let r = off_rt(|| revoke_session("ftp://x", "tok", true, REVOKE_SESSION_BUDGET)).await;
    assert!(matches!(r, Err(FsyncError::Invalid(_))), "{r:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_network_is_a_bounded_retryable_error_then_it_works() {
    let be = Backend::start().await;
    let t = login(&be).await.api.tokens().unwrap();
    let mut proxy = Proxy::up(be.base.trim_start_matches("http://").parse().unwrap()).await;
    let via = format!("http://{}", proxy.addr);

    proxy.stop().await; // refused
    let r = revoke(&via, &t.refresh, REVOKE_SESSION_BUDGET).await;
    assert!(matches!(r, Err(FsyncError::Network(_))), "refused: {r:?}");

    proxy.start(Mode::Blackhole).await; // accepted, never answered
    let started = Instant::now();
    let r = revoke(&via, &t.refresh, Duration::from_secs(1)).await;
    assert!(
        matches!(r, Err(FsyncError::Network(_))),
        "black hole: {r:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "{:?}",
        started.elapsed()
    );
    assert_ne!(
        status(&be.base, &t.access).await,
        401,
        "still live: not revoked"
    );

    proxy.stop().await;
    proxy.start(Mode::Forward).await;
    revoke(&via, &t.refresh, REVOKE_SESSION_BUDGET)
        .await
        .expect("backend back");
    assert_eq!(status(&be.base, &t.access).await, 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn revoke_session_from_inside_a_runtime_is_a_plain_error() {
    let r = revoke_session("http://127.0.0.1:1", "tok", true, Duration::from_secs(1));
    assert!(matches!(r, Err(FsyncError::Internal(_))), "{r:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn logout_confirmed_reports_whether_the_revocation_settled() {
    let be = Backend::start().await;
    let d = dev_with(&be, 7, "confirm", false);
    let (d, t, confirmed, again) = off_rt({
        let be_email = be.email.clone();
        move || {
            d.svc.login_start(service_harness::EMAIL).unwrap();
            let code = be_email
                .0
                .lock()
                .unwrap()
                .get(service_harness::EMAIL)
                .cloned()
                .unwrap();
            let t = d.svc.login_verify(service_harness::EMAIL, &code).unwrap();
            let confirmed = d.svc.logout_confirmed().unwrap();
            // Nothing held any more: nothing was revoked by this call.
            let again = d.svc.logout_confirmed().unwrap();
            (d, t, confirmed, again)
        }
    })
    .await;
    assert!(confirmed, "backend answered: settled");
    assert!(!again, "no session to revoke is not a confirmation");
    assert_eq!(status(&be.base, &t.access).await, 401);
    off_rt(move || drop(d)).await;
}
