//! Backend HTTP client: bearer auth with refresh-on-401 and a token-persistence callback.

mod authed;
pub(crate) use super::check::check;
use super::{
    limits::{capped_json, check_scheme},
    tls::client_config,
};
use crate::error::{CoreError, Result};
use fsync_proto::api_dto::{Refresh, TokenPair};
use reqwest::Client;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

/// Called with every new token pair so the app can persist it (Keychain/DPAPI). Runs while the
/// client holds its token lock: it must be quick and must not call back into the client.
pub type TokenCallback = Arc<dyn Fn(&TokenPair) + Send + Sync>;

pub struct ApiClient {
    base: String,
    http: Client,
    tokens: Mutex<Option<TokenPair>>,
    refresh_lock: tokio::sync::Mutex<()>,
    /// Bumped by `take_refresh`: a refresh that started earlier must not store its result.
    epoch: AtomicU64,
    on_tokens: Option<TokenCallback>,
}

impl ApiClient {
    /// `base` like `https://api.example.com` (no trailing slash needed).
    pub fn new(
        base: &str,
        tokens: Option<TokenPair>,
        on_tokens: Option<TokenCallback>,
    ) -> Result<Self> {
        Self::with_options(base, tokens, on_tokens, false)
    }

    /// Like `new`; `allow_insecure_http` (dev/test backends only) lets a plain `http://` base
    /// through. Anything but http(s) is always refused.
    pub fn with_options(
        base: &str,
        tokens: Option<TokenPair>,
        on_tokens: Option<TokenCallback>,
        allow_insecure_http: bool,
    ) -> Result<Self> {
        check_scheme(base, allow_insecure_http)?;
        let http = Client::builder()
            .use_preconfigured_tls((*client_config()?).clone())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| CoreError::Http(e.to_string()))?;
        Ok(Self {
            base: base.trim_end_matches('/').to_owned(),
            http,
            tokens: Mutex::new(tokens),
            refresh_lock: tokio::sync::Mutex::new(()),
            epoch: AtomicU64::new(0),
            on_tokens,
        })
    }

    pub(crate) fn base(&self) -> &str {
        &self.base
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}/v1/{path}", self.base)
    }

    pub fn tokens(&self) -> Option<TokenPair> {
        self.tokens.lock().ok().and_then(|t| t.clone())
    }

    pub fn access_token(&self) -> Result<String> {
        self.tokens()
            .map(|t| t.access)
            .ok_or(CoreError::AuthExpired)
    }

    /// Install tokens the app obtained earlier (no persistence callback: it already has them).
    pub fn set_session(&self, t: TokenPair) {
        if let Ok(mut g) = self.tokens.lock() {
            *g = Some(t);
        }
    }

    pub(crate) fn set_tokens(&self, t: TokenPair) {
        if let Some(cb) = &self.on_tokens {
            cb(&t);
        }
        if let Ok(mut g) = self.tokens.lock() {
            *g = Some(t);
        }
    }

    pub(crate) fn clear_tokens(&self) {
        if let Ok(mut g) = self.tokens.lock() {
            *g = None;
        }
    }

    /// Store the result of a refresh that began at `epoch`, unless a logout happened since.
    /// The check, the store and the app callback share one critical section, so a logout
    /// either precedes all of it (nothing is stored or reported) or follows it.
    fn set_tokens_at(&self, epoch: u64, t: TokenPair) -> Result<()> {
        let mut g = self.tokens.lock().map_err(|_| CoreError::Closed)?;
        if self.epoch.load(Ordering::SeqCst) != epoch {
            return Err(CoreError::AuthExpired);
        }
        if let Some(cb) = &self.on_tokens {
            cb(&t);
        }
        *g = Some(t);
        Ok(())
    }

    /// Logout: forget the tokens, hand back the refresh token, and invalidate refreshes in
    /// flight. A poisoned lock is recovered (the tokens must go regardless).
    pub fn take_refresh(&self) -> Option<String> {
        let mut g = self.tokens.lock().unwrap_or_else(|p| {
            tracing::warn!("token lock poisoned during logout; clearing anyway");
            p.into_inner()
        });
        self.epoch.fetch_add(1, Ordering::SeqCst);
        g.take().map(|t| t.refresh)
    }

    pub(crate) fn plain(&self) -> &Client {
        &self.http
    }

    /// Refresh unless another task already replaced `rejected`.
    async fn refresh_after(&self, rejected: &str) -> Result<()> {
        let _single_flight = self.refresh_lock.lock().await;
        if self.tokens().is_some_and(|t| t.access != rejected) {
            return Ok(());
        }
        self.refresh_locked().await
    }

    /// Force a refresh (presence reconnect after token expiry).
    pub async fn refresh_now(&self) -> Result<()> {
        let _single_flight = self.refresh_lock.lock().await;
        self.refresh_locked().await
    }

    async fn refresh_locked(&self) -> Result<()> {
        let epoch = self.epoch.load(Ordering::SeqCst);
        for attempt in 0..4u64 {
            let refresh = self.tokens().ok_or(CoreError::AuthExpired)?.refresh;
            let resp = self
                .http
                .post(self.url("auth/refresh"))
                .json(&Refresh { refresh })
                .send()
                .await;
            match check(resp).await {
                Ok(r) => {
                    let t: TokenPair = capped_json(r).await?;
                    return self.set_tokens_at(epoch, t);
                }
                // Rotated < 10 s ago elsewhere: wait for the newer token, then retry.
                Err(CoreError::Api(e)) if e.is("refresh_in_progress") => {
                    tokio::time::sleep(Duration::from_millis(500 * (attempt + 1))).await;
                }
                Err(CoreError::Api(e)) if e.status == 401 => {
                    if self.epoch.load(Ordering::SeqCst) == epoch {
                        self.clear_tokens();
                    }
                    return Err(CoreError::AuthExpired);
                }
                Err(e) => return Err(e),
            }
        }
        Err(CoreError::Http("refresh kept conflicting".into()))
    }
}
