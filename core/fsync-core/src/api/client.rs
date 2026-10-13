//! Backend HTTP client: bearer auth with refresh-on-401 and a token-persistence callback.

use super::{
    limits::{capped_json, check_scheme},
    tls::client_config,
};
use crate::error::{ApiError, CoreError, Result};
use fsync_proto::api_dto::{ApiErrorBody, Refresh, TokenPair};
use reqwest::{Client, RequestBuilder, Response};
use serde::de::DeserializeOwned;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

/// Called with every new token pair so the app can persist it (Keychain/DPAPI).
pub type TokenCallback = Arc<dyn Fn(&TokenPair) + Send + Sync>;

pub struct ApiClient {
    base: String,
    http: Client,
    tokens: Mutex<Option<TokenPair>>,
    refresh_lock: tokio::sync::Mutex<()>,
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

    pub(crate) fn plain(&self) -> &Client {
        &self.http
    }

    /// Authenticated request; on 401 (not `reauth_required`) refresh once and retry.
    pub(crate) async fn authed(
        &self,
        build: impl Fn(&Client, &str) -> RequestBuilder,
    ) -> Result<Response> {
        let access = self.access_token()?;
        match check(build(&self.http, &access).send().await).await {
            Err(CoreError::Api(e)) if e.status == 401 && !e.is("reauth_required") => {
                self.refresh_after(&access).await?;
                let access = self.access_token()?;
                check(build(&self.http, &access).send().await).await
            }
            other => other,
        }
    }

    pub(crate) async fn authed_json<T: DeserializeOwned>(
        &self,
        build: impl Fn(&Client, &str) -> RequestBuilder,
    ) -> Result<T> {
        capped_json(self.authed(build).await?).await
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
                    self.set_tokens(t);
                    return Ok(());
                }
                // Rotated < 10 s ago elsewhere: wait for the newer token, then retry.
                Err(CoreError::Api(e)) if e.is("refresh_in_progress") => {
                    tokio::time::sleep(Duration::from_millis(500 * (attempt + 1))).await;
                }
                Err(CoreError::Api(e)) if e.status == 401 => {
                    self.clear_tokens();
                    return Err(CoreError::AuthExpired);
                }
                Err(e) => return Err(e),
            }
        }
        Err(CoreError::Http("refresh kept conflicting".into()))
    }
}

/// Map transport errors and non-2xx bodies to `CoreError`.
pub(crate) async fn check(r: reqwest::Result<Response>) -> Result<Response> {
    let resp = r.map_err(|e| CoreError::Http(e.without_url().to_string()))?;
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status().as_u16();
    let body = capped_json::<ApiErrorBody>(resp).await.ok();
    Err(CoreError::Api(ApiError {
        status,
        code: body
            .as_ref()
            .map_or_else(|| "unknown".into(), |b| b.code.clone()),
        message: body.map(|b| b.message).unwrap_or_default(),
    }))
}
