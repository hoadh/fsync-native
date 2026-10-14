//! Real fsync-backend on a random port with a throwaway Postgres (testcontainers), one DB per test.
#![allow(dead_code)]

use async_trait::async_trait;
use fsync_backend::{
    build_router,
    config::Config,
    db,
    email::sender::{EmailError, EmailSender},
    presence::hub::InMemoryHub,
    state::AppState,
};
use sqlx::{Connection, PgConnection};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use uuid::Uuid;

// One Postgres helper for both crates (container start, port-publish retry, atexit cleanup).
#[path = "../../../../backend/tests/common/pg.rs"]
mod pg;

#[derive(Default)]
pub struct CaptureSender(pub Mutex<HashMap<String, String>>);

#[async_trait]
impl EmailSender for CaptureSender {
    async fn send_otp(&self, to: &str, code: &str) -> Result<(), EmailError> {
        self.0.lock().unwrap().insert(to.into(), code.into());
        Ok(())
    }
}

pub struct Backend {
    pub base: String,
    pub email: Arc<CaptureSender>,
}

impl Backend {
    pub async fn start() -> Self {
        let admin = pg::admin_url().await;
        let name = format!("t_{}", Uuid::new_v4().simple());
        let mut conn = {
            let mut attempt = 0;
            loop {
                match PgConnection::connect(&format!("{admin}/postgres")).await {
                    Ok(c) => break c,
                    Err(_) if attempt < 40 => {
                        attempt += 1;
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
        let email = Arc::new(CaptureSender::default());
        let state = AppState::new(pool, cfg, email.clone(), Arc::new(InMemoryHub::default()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = build_router(state).into_make_service_with_connect_info::<SocketAddr>();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            base: format!("http://{addr}"),
            email,
        }
    }

    pub fn last_code(&self, email: &str) -> String {
        self.email.0.lock().unwrap().get(email).cloned().unwrap()
    }
}

const EMAIL: &str = "dev@example.com";
use crate::common::Peer;
use fsync_core::{
    CoreError,
    api::{ApiClient, PresenceEvent, PresenceHandle, TokenCallback, spawn_presence},
    net::AddrFeed,
};
use fsync_proto::presence::Addrs;
use tokio::sync::mpsc;

pub struct Login {
    pub api: Arc<ApiClient>,
    pub account: Uuid,
    pub refreshed: Arc<Mutex<Vec<String>>>,
}

pub async fn login(be: &Backend) -> Login {
    let refreshed = Arc::new(Mutex::new(Vec::new()));
    let r2 = refreshed.clone();
    let cb: TokenCallback = Arc::new(move |t| r2.lock().unwrap().push(t.refresh.clone()));
    let api = Arc::new(ApiClient::with_options(&be.base, None, Some(cb), true).unwrap());
    api.otp_start(EMAIL).await.unwrap();
    let account = api.otp_verify(EMAIL, &be.last_code(EMAIL)).await.unwrap();
    Login {
        api,
        account,
        refreshed,
    }
}

pub fn start_presence(
    api: &Arc<ApiClient>,
    p: &Peer,
) -> (PresenceHandle, mpsc::Receiver<PresenceEvent>) {
    let (tx, rx) = mpsc::channel(64);
    let addr = p.loopback();
    let (addrs_tx, rx_addrs) = tokio::sync::watch::channel(Addrs {
        relay_url: None,
        direct: vec![addr],
    });
    drop(addrs_tx); // the address never changes
    let feed = AddrFeed {
        rx: rx_addrs,
        expect_relay: false,
    };
    (spawn_presence(api.clone(), p.id.clone(), feed, tx), rx)
}

pub async fn until<T>(
    rx: &mut mpsc::Receiver<PresenceEvent>,
    f: impl Fn(&PresenceEvent) -> Option<T>,
) -> T {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(v) = f(&rx.recv().await.expect("presence closed")) {
                return v;
            }
        }
    })
    .await
    .expect("presence timeout")
}

pub fn api_code(e: CoreError) -> (u16, String) {
    match e {
        CoreError::Api(a) => (a.status, a.code),
        other => panic!("expected api error, got {other:?}"),
    }
}
