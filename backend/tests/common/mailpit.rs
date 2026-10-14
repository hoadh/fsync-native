//! Real local SMTP server for tests: a pinned Mailpit container (like `pg.rs` does for Postgres).
//!
//! `tls()` is a server that, like Gmail, only talks after STARTTLS and requires a login; its
//! certificate is generated per run (rcgen) and handed to the client as an extra root.
//! `plain()` offers no TLS at all, to prove the sender refuses to downgrade.
#![allow(dead_code)]

use fsync_backend::email::config::SmtpConfig;
use lettre::message::Mailbox;
use serde_json::Value;
use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};
use tokio::sync::OnceCell;
use zeroize::Zeroizing;

/// Pinned: the test must not drift with `latest`.
const IMAGE_TAG: &str = "v1.31.5";
/// Docker Desktop can be slow under load; the default 60 s is tight.
const STARTUP: std::time::Duration = std::time::Duration::from_secs(120);
pub const USER: &str = "fsync-bot";
pub const PASSWORD: &str = "app-password-1234";

/// Container ids for the atexit cleanup (statics are never dropped; testcontainers-rs has no reaper).
static IDS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

extern "C" fn remove_containers() {
    if let Ok(ids) = IDS.lock() {
        for id in ids.iter() {
            let _ = std::process::Command::new("docker")
                .args(["rm", "-f", "-v", id])
                .stdout(std::process::Stdio::null())
                .status();
        }
    }
}

pub struct Mailpit {
    _container: ContainerAsync<GenericImage>,
    smtp_port: u16,
    http: String,
    /// DER of the server certificate (`tls()` only); the client trusts exactly this.
    pub cert_der: Vec<u8>,
}

static TLS: OnceCell<Mailpit> = OnceCell::const_new();
static PLAIN: OnceCell<Mailpit> = OnceCell::const_new();

/// Shared by every test of the binary: STARTTLS required, login required.
pub async fn tls() -> &'static Mailpit {
    TLS.get_or_init(|| Mailpit::start(true)).await
}

/// Shared: no TLS offered.
pub async fn plain() -> &'static Mailpit {
    PLAIN.get_or_init(|| Mailpit::start(false)).await
}

impl Mailpit {
    async fn start(with_tls: bool) -> Self {
        let img = GenericImage::new("axllent/mailpit", IMAGE_TAG)
            .with_exposed_port(1025.tcp())
            .with_exposed_port(8025.tcp())
            .with_wait_for(WaitFor::message_on_stdout("accessible via"));
        let mut cert_der = Vec::new();
        let img = if with_tls {
            let ck = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
            cert_der = ck.cert.der().to_vec();
            img.with_copy_to("/tls/cert.pem", ck.cert.pem().into_bytes())
                .with_copy_to("/tls/key.pem", ck.signing_key.serialize_pem().into_bytes())
                .with_copy_to("/tls/auth.txt", format!("{USER}:{PASSWORD}\n").into_bytes())
                .with_env_var("MP_SMTP_TLS_CERT", "/tls/cert.pem")
                .with_env_var("MP_SMTP_TLS_KEY", "/tls/key.pem")
                .with_env_var("MP_SMTP_REQUIRE_STARTTLS", "1")
                .with_env_var("MP_SMTP_AUTH_FILE", "/tls/auth.txt")
                .with_startup_timeout(STARTUP)
        } else {
            img // no TLS, no auth: the sender must refuse it
                .with_startup_timeout(STARTUP)
        };
        let c = img
            .start()
            .await
            .expect("start mailpit container (is Docker running?)");
        // SAFETY: registers a plain `extern "C" fn` with no captured state.
        IDS.lock().unwrap().push(c.id().to_string());
        unsafe { libc::atexit(remove_containers) };
        let smtp_port = c.get_host_port_ipv4(1025).await.expect("smtp port");
        let http_port = c.get_host_port_ipv4(8025).await.expect("http port");
        let mp = Self {
            _container: c,
            smtp_port,
            http: format!("http://127.0.0.1:{http_port}"),
            cert_der,
        };
        mp.wait_ready().await;
        mp
    }

    /// The log line precedes readiness of the published ports under load (Docker Desktop port
    /// forwarding), so wait for a real SMTP greeting and a real HTTP answer, with a deadline.
    async fn wait_ready(&self) {
        use tokio::io::AsyncReadExt;
        let deadline = std::time::Instant::now() + STARTUP;
        let http = reqwest::Client::new();
        let (mut smtp_ok, mut http_ok) = (false, false);
        while !(smtp_ok && http_ok) {
            assert!(
                std::time::Instant::now() < deadline,
                "mailpit not ready: smtp={smtp_ok} http={http_ok}"
            );
            if !smtp_ok {
                let probe = async {
                    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", self.smtp_port))
                        .await
                        .ok()?;
                    let mut buf = [0u8; 4];
                    s.read_exact(&mut buf).await.ok()?;
                    (&buf == b"220 ").then_some(())
                };
                smtp_ok = tokio::time::timeout(std::time::Duration::from_secs(3), probe)
                    .await
                    .is_ok_and(|r| r.is_some());
            }
            if !http_ok {
                http_ok = http
                    .get(format!("{}/api/v1/info", self.http))
                    .timeout(std::time::Duration::from_secs(3))
                    .send()
                    .await
                    .is_ok_and(|r| r.status().is_success());
            }
            if !(smtp_ok && http_ok) {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
        }
    }

    /// Client settings that match this server (`password` lets tests try a wrong one).
    pub fn smtp_config(&self, password: &str) -> SmtpConfig {
        SmtpConfig {
            host: "127.0.0.1".into(),
            port: self.smtp_port,
            username: USER.into(),
            password: Zeroizing::new(password.into()),
            from: "fsync <noreply@fsync.test>".parse::<Mailbox>().unwrap(),
        }
    }

    /// Mails delivered to `to` (Mailpit search API), polled until one shows up or 20 s pass.
    pub async fn messages_to(&self, to: &str) -> Vec<Value> {
        let http = reqwest::Client::new();
        for _ in 0..100 {
            let v: Value = http
                .get(format!("{}/api/v1/search", self.http))
                .query(&[("query", format!("to:{to}"))])
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            let list = v["messages"].as_array().cloned().unwrap_or_default();
            if !list.is_empty() {
                return list;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        Vec::new()
    }

    /// Full message (headers and bodies) by the id from `messages_to`.
    pub async fn message(&self, id: &str) -> Value {
        reqwest::get(format!("{}/api/v1/message/{id}", self.http))
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }
}
