//! `logout()` revokes the refresh token server-side, also when the backend is unreachable at
//! that moment (refused or black-holed), without blocking the caller for long; and a logout
//! wins against a token refresh that is still in flight. A TCP forwarder in front of the real
//! backend is cut, black-holed and gated.

mod backend_harness;
mod common;
use backend_harness::{
    Backend,
    proxy::{Mode, Proxy},
};
use fsync_core::{CoreError, api::ApiClient, api::TokenCallback, service::*};
use fsync_proto::Platform;
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const EMAIL: &str = "dev@example.com";

struct Nop;
impl ServiceDelegate for Nop {
    fn on_remote_clip(&self, _: ClipItem) {}
    fn on_device_status(&self, _: Device) {}
    fn on_transfer(&self, _: TransferEvent) {}
    fn on_incoming_files(&self, _: ClipItem) {}
    fn on_pairing_event(&self, _: PairingEvent) {}
    fn on_account_event(&self, _: AccountEvent) {}
    fn on_session_changed(&self, _: SessionTokens) {}
    fn on_error(&self, _: FsyncError) {}
}

/// Service calls are blocking and refuse to run on a runtime thread: use a plain thread.
async fn off_rt<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    // Bounded: a wedged service call fails the test (with the thread still visible in a
    // sample) instead of hanging the whole suite.
    tokio::time::timeout(Duration::from_secs(90), rx)
        .await
        .expect("service call did not return within 90 s")
        .expect("service thread panicked")
}

/// Status of an authenticated call with `access` made straight against the backend.
async fn status(base: &str, access: &str) -> u16 {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
        .get(format!("{base}/v1/device-list"))
        .bearer_auth(access)
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

fn addr_of(be: &Backend) -> SocketAddr {
    be.base.trim_start_matches("http://").parse().unwrap()
}

/// Logout while the backend is down in the way `down` describes; then it comes back.
async fn offline_logout(down: Mode) {
    let be = Backend::start().await;
    let mut proxy = Proxy::up(addr_of(&be)).await;
    let direct = be.base.clone();
    let dir = tempfile::tempdir().unwrap();
    let cfg = ServiceConfig {
        data_dir: dir.path().to_owned(),
        api_base: format!("http://{}", proxy.addr),
        relay_urls: vec![],
        device_name: "logout-test".into(),
        platform: Platform::Mac,
        device_secret: Zeroizing::new([3; 32]),
        history_key: Zeroizing::new([4; 32]),
        force_relay: false,
        allow_insecure_http: true,
        mdns: false,
    };
    let email = be.email.clone();
    let (svc, tokens) = off_rt(move || {
        let svc = FsyncService::new(cfg, Arc::new(Nop)).unwrap();
        svc.set_revoke_retry_base(Duration::from_millis(100));
        svc.login_start(EMAIL).unwrap();
        let code = email.0.lock().unwrap().get(EMAIL).cloned().unwrap();
        let t = svc.login_verify(EMAIL, &code).unwrap();
        (svc, t)
    })
    .await;
    assert_ne!(
        status(&direct, &tokens.access).await,
        401,
        "session is live"
    );

    proxy.stop().await;
    if down == Mode::Blackhole {
        proxy.start(Mode::Blackhole).await;
    }
    let svc = off_rt(move || {
        let t = Instant::now();
        let settled = svc
            .logout_confirmed()
            .expect("local sign-out succeeds offline");
        assert!(!settled, "unreachable backend: revocation only queued");
        // Apps may call this on a UI thread: bounded, whatever the network does.
        assert!(t.elapsed() < Duration::from_secs(6), "{:?}", t.elapsed());
        assert!(svc.devices().is_empty());
        svc
    })
    .await;
    assert_ne!(
        status(&direct, &tokens.access).await,
        401,
        "not reached yet"
    );

    proxy.stop().await;
    proxy.start(Mode::Forward).await;
    let end = Instant::now() + Duration::from_secs(20);
    while status(&direct, &tokens.access).await != 401 {
        assert!(Instant::now() < end, "revocation never arrived");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    off_rt(move || drop(svc)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn logout_with_refused_connections_revokes_when_the_backend_returns() {
    offline_logout(Mode::Forward).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn logout_against_a_black_holed_backend_returns_fast_and_revokes_later() {
    offline_logout(Mode::Blackhole).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refresh_in_flight_cannot_store_tokens_after_logout() {
    let be = Backend::start().await;
    let proxy = Proxy::up(addr_of(&be)).await;
    let stored: Arc<Mutex<Vec<String>>> = Arc::default();
    let s2 = stored.clone();
    let cb: TokenCallback = Arc::new(move |t| s2.lock().unwrap().push(t.refresh.clone()));
    let api = Arc::new(
        ApiClient::with_options(&format!("http://{}", proxy.addr), None, Some(cb), true).unwrap(),
    );
    api.otp_start(EMAIL).await.unwrap();
    api.otp_verify(EMAIL, &be.last_code(EMAIL)).await.unwrap();
    assert_eq!(stored.lock().unwrap().len(), 1);

    // New connections stall at the proxy, so the refresh is in flight while we log out.
    proxy.set_gate(false);
    api.set_session(api.tokens().unwrap());
    let a2 = api.clone();
    let refresh = tokio::spawn(async move { a2.refresh_now().await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    // The pooled keep-alive connection may carry it; make sure the request is really pending.
    assert!(!refresh.is_finished(), "refresh must still be in flight");
    let old = api.take_refresh().expect("tokens were present");
    proxy.set_gate(true);

    let r = refresh.await.unwrap();
    assert!(matches!(r, Err(CoreError::AuthExpired)), "{r:?}");
    assert!(api.tokens().is_none(), "no session after logout");
    assert_eq!(
        stored.lock().unwrap().len(),
        1,
        "app was not handed a new session"
    );
    api.revoke(&old).await.unwrap();
}
