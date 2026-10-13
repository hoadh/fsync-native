//! Shared application state.

use crate::{
    auth::tokens::TokenService,
    config::Config,
    email::sender::EmailSender,
    presence::{gate::UnauthGate, hub::PresenceHub},
    rate_limit::RateLimiter,
};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Semaphore;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub cfg: Arc<Config>,
    pub tokens: Arc<TokenService>,
    pub email: Arc<dyn EmailSender>,
    pub hub: Arc<dyn PresenceHub>,
    pub limiter: Arc<RateLimiter>,
    /// Key for hashing low-entropy codes (derived from the signing seed).
    pub hash_key: Arc<Zeroizing<[u8; 32]>>,
    /// Permits for concurrently processed HTTP requests.
    pub in_flight: Arc<Semaphore>,
    /// Caps sockets that have not completed the presence handshake.
    pub unauth: Arc<UnauthGate>,
}

impl AppState {
    pub fn new(
        db: PgPool,
        cfg: Config,
        email: Arc<dyn EmailSender>,
        hub: Arc<dyn PresenceHub>,
    ) -> Self {
        let hash_key = Zeroizing::new(blake3::derive_key(
            "fsync-backend 2026 code-hash",
            cfg.signing_seed.as_slice(),
        ));
        Self {
            in_flight: Arc::new(Semaphore::new(cfg.max_in_flight)),
            unauth: Arc::new(UnauthGate::new(cfg.max_unauth_ws, cfg.max_unauth_ws_per_ip)),
            db,
            tokens: Arc::new(TokenService::new(&cfg.signing_seed)),
            cfg: Arc::new(cfg),
            email,
            hub,
            limiter: Arc::new(RateLimiter::default()),
            hash_key: Arc::new(hash_key),
        }
    }
}
