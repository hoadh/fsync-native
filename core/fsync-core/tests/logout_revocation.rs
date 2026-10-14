//! `logout()` revokes the refresh token server-side, also when the backend is unreachable at
//! that moment. A TCP forwarder in front of the real backend is switched off and on.

mod backend_harness;
mod common;
use backend_harness::Backend;
use fsync_core::service::*;
use fsync_proto::Platform;
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{net::TcpListener, task::JoinHandle};
use zeroize::Zeroizing;

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

/// Forwards connections on `addr` to `target`; `stop` frees the port (connection refused).
struct Proxy {
    addr: SocketAddr,
    target: SocketAddr,
    task: Option<JoinHandle<()>>,
}

impl Proxy {
    async fn up(target: SocketAddr) -> Self {
        let mut p = Self {
            addr: "127.0.0.1:0".parse().unwrap(),
            target,
            task: None,
        };
        p.start().await;
        p
    }

    async fn start(&mut self) {
        let l = TcpListener::bind(self.addr).await.unwrap();
        self.addr = l.local_addr().unwrap();
        let target = self.target;
        self.task = Some(tokio::spawn(async move {
            // Live connections belong to this task: stopping the proxy cuts them too.
            let mut conns = tokio::task::JoinSet::new();
            while let Ok((mut c, _)) = l.accept().await {
                conns.spawn(async move {
                    if let Ok(mut s) = tokio::net::TcpStream::connect(target).await {
                        let _ = tokio::io::copy_bidirectional(&mut c, &mut s).await;
                    }
                });
            }
        }));
    }

    async fn stop(&mut self) {
        if let Some(t) = self.task.take() {
            t.abort();
            let _ = t.await;
        }
    }
}

/// Service calls are blocking and refuse to run on a runtime thread: use a plain thread.
async fn off_rt<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.await.expect("service thread panicked")
}

/// Status of an authenticated call with `access` made straight against the backend.
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

#[tokio::test(flavor = "multi_thread")]
async fn offline_logout_signs_out_locally_and_revokes_when_the_backend_returns() {
    let be = Backend::start().await;
    let target: SocketAddr = be.base.trim_start_matches("http://").parse().unwrap();
    let mut proxy = Proxy::up(target).await;
    let (base, direct) = (format!("http://{}", proxy.addr), be.base.clone());
    let dir = tempfile::tempdir().unwrap();
    let cfg = ServiceConfig {
        data_dir: dir.path().to_owned(),
        api_base: base,
        relay_url: None,
        device_name: "logout-test".into(),
        platform: Platform::Mac,
        device_secret: Zeroizing::new([3; 32]),
        history_key: Zeroizing::new([4; 32]),
        force_relay: false,
        allow_insecure_http: true,
        mdns: false,
    };
    let tokens = off_rt({
        let email = be.email.clone();
        move || {
            let svc = FsyncService::new(cfg, Arc::new(Nop)).unwrap();
            svc.set_revoke_retry_base(Duration::from_millis(100));
            svc.login_start("dev@example.com").unwrap();
            let code = email
                .0
                .lock()
                .unwrap()
                .get("dev@example.com")
                .cloned()
                .unwrap();
            let t = svc.login_verify("dev@example.com", &code).unwrap();
            (svc, t)
        }
    })
    .await;
    let (svc, tokens) = tokens;
    assert_ne!(
        status(&direct, &tokens.access).await,
        401,
        "session is live"
    );

    proxy.stop().await;
    let svc = off_rt(move || {
        let t = Instant::now();
        svc.logout().expect("local sign-out succeeds offline");
        assert!(t.elapsed() < Duration::from_secs(15), "{:?}", t.elapsed());
        svc
    })
    .await;
    assert_ne!(
        status(&direct, &tokens.access).await,
        401,
        "unreachable: still valid"
    );

    proxy.start().await;
    let end = Instant::now() + Duration::from_secs(15);
    while status(&direct, &tokens.access).await != 401 {
        assert!(Instant::now() < end, "revocation never arrived");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    off_rt(move || drop(svc)).await;
}
