//! Shared integration-test harness: throwaway Postgres (testcontainers), one DB per test,
//! real HTTP server on a random port, captured OTP emails.
#![allow(dead_code)]

pub mod devices;
pub mod mailpit;
pub mod pg;
pub mod ws;

use async_trait::async_trait;
use fsync_backend::{
    build_router,
    config::Config,
    db,
    email::sender::{EmailError, EmailSender},
    presence::hub::InMemoryHub,
    state::AppState,
};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, PgPool};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;

/// Captures the last OTP per recipient; `fail` makes every send return an error.
#[derive(Default)]
pub struct CaptureSender {
    pub codes: Mutex<HashMap<String, String>>,
    pub fail: AtomicBool,
}

#[async_trait]
impl EmailSender for CaptureSender {
    async fn send_otp(&self, to: &str, code: &str) -> Result<(), EmailError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(EmailError("provider down".into()));
        }
        self.codes.lock().unwrap().insert(to.into(), code.into());
        Ok(())
    }
}

pub struct Session {
    pub access: String,
    pub refresh: String,
    pub account_id: Uuid,
}

pub struct TestApp {
    pub base: String,
    pub addr: SocketAddr,
    pub state: AppState,
    pub pool: PgPool,
    pub email: Arc<CaptureSender>,
    pub http: reqwest::Client,
}

impl TestApp {
    pub async fn start() -> Self {
        Self::start_with(|_| {}).await
    }

    pub async fn start_with(tweak: impl FnOnce(&mut Config)) -> Self {
        Self::start_custom(tweak, None).await
    }

    /// Same app, but OTP emails go through `sender` (e.g. a real SMTP sender) instead of the
    /// capture sink; `email` then stays empty.
    pub async fn start_with_sender(sender: Arc<dyn EmailSender>) -> Self {
        Self::start_custom(|_| {}, Some(sender)).await
    }

    async fn start_custom(
        tweak: impl FnOnce(&mut Config),
        sender: Option<Arc<dyn EmailSender>>,
    ) -> Self {
        let admin = pg::admin_url().await;
        let name = format!("t_{}", Uuid::new_v4().simple());
        // The server may still be finishing init right after the readiness line: retry.
        let mut conn = {
            let mut attempt = 0;
            loop {
                match PgConnection::connect(&format!("{admin}/postgres")).await {
                    Ok(c) => break c,
                    Err(e) if attempt < 40 => {
                        attempt += 1;
                        let _ = e;
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                    Err(e) => panic!("connect admin: {e}"),
                }
            }
        };
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&mut conn)
            .await
            .unwrap();
        let url = format!("{admin}/{name}");
        let pool = db::connect(&url).await.unwrap();
        db::migrate(&pool).await.unwrap();

        let mut cfg = Config::with_defaults(url, [42; 32]);
        cfg.bind_addr = "127.0.0.1:0".parse().unwrap();
        cfg.dev_mode = true;
        tweak(&mut cfg);
        let email = Arc::new(CaptureSender::default());
        let state = AppState::new(
            pool.clone(),
            cfg,
            sender.unwrap_or_else(|| -> Arc<dyn EmailSender> { email.clone() }),
            Arc::new(InMemoryHub::default()),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = build_router(state.clone()).into_make_service_with_connect_info::<SocketAddr>();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            base: format!("http://{addr}"),
            addr,
            state,
            pool,
            email,
            http: reqwest::Client::new(),
        }
    }

    pub async fn refresh(&self, token: &str) -> reqwest::Response {
        self.http
            .post(self.url("/v1/auth/refresh"))
            .json(&json!({ "refresh": token }))
            .send()
            .await
            .unwrap()
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    pub async fn start_otp(&self, email: &str) -> reqwest::Response {
        self.http
            .post(self.url("/v1/auth/otp/start"))
            .json(&json!({ "email": email }))
            .send()
            .await
            .unwrap()
    }

    pub fn last_code(&self, email: &str) -> String {
        self.email
            .codes
            .lock()
            .unwrap()
            .get(email)
            .cloned()
            .unwrap()
    }

    pub async fn verify_otp(&self, email: &str, code: &str) -> reqwest::Response {
        self.http
            .post(self.url("/v1/auth/otp/verify"))
            .json(&json!({ "email": email, "code": code }))
            .send()
            .await
            .unwrap()
    }

    /// Full OTP login for `email` (lowercase).
    pub async fn login(&self, email: &str) -> Session {
        assert_eq!(self.start_otp(email).await.status(), 204);
        let code = self.last_code(email);
        let resp = self.verify_otp(email, &code).await;
        assert_eq!(resp.status(), 200);
        let v: Value = resp.json().await.unwrap();
        Session {
            access: v["access"].as_str().unwrap().into(),
            refresh: v["refresh"].as_str().unwrap().into(),
            account_id: v["account_id"].as_str().unwrap().parse().unwrap(),
        }
    }
}
