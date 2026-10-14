//! Sign-in and session handling. Tokens live in the app's keystore, never in core files.

use super::{
    FsyncService,
    delegate::Out,
    error::{FsyncError, SvcResult},
    inner::{Inner, lock},
    types::SessionTokens,
};
use fsync_proto::api_dto::TokenPair;
use std::sync::Arc;
use uuid::Uuid;

impl Inner {
    fn set_account(&self, id: Option<Uuid>) -> SvcResult<()> {
        *lock(&self.account)? = id;
        Ok(())
    }

    pub(super) async fn login_verify(
        self: &Arc<Self>,
        email: &str,
        code: &str,
    ) -> SvcResult<SessionTokens> {
        let id = self.api.otp_verify(email, code).await?;
        self.set_account(Some(id))?;
        let t = self
            .api
            .tokens()
            .ok_or_else(|| FsyncError::Internal("no tokens after login".into()))?;
        let tokens = SessionTokens {
            access: t.access,
            refresh: t.refresh,
            account_id: id.to_string(),
        };
        self.emit(Out::Session(tokens.clone()));
        self.restart_presence()?;
        Ok(tokens)
    }

    pub(super) fn set_session(self: &Arc<Self>, t: SessionTokens) -> SvcResult<()> {
        let id = Uuid::parse_str(&t.account_id)
            .map_err(|_| FsyncError::invalid("malformed account id"))?;
        if t.access.is_empty() || t.refresh.is_empty() {
            return Err(FsyncError::invalid("empty session token"));
        }
        self.set_account(Some(id))?;
        self.api.set_session(TokenPair {
            access: t.access,
            refresh: t.refresh,
        });
        self.restart_presence()
    }

    fn sign_out_locally(&self) -> SvcResult<()> {
        let account = self.set_account(None);
        let presence = lock(&self.live).map(|mut l| l.presence = None);
        let trust = self.trust.clear().map_err(FsyncError::from);
        account.and(presence).and(trust)
    }

    /// Signs out locally first (never blocked by the network), then revokes the refresh token
    /// server-side: retried briefly inline, then in the background (see `revoke.rs`).
    /// Returns whether the server-side revocation settled; `false` also when there was no
    /// session to revoke (the app's own copy of a token may still be live).
    pub(super) async fn logout(self: &Arc<Self>) -> SvcResult<bool> {
        // Take the token first: a later local failure must not leave the session unrevoked.
        let refresh = self.api.take_refresh();
        let local = self.sign_out_locally();
        let settled = match refresh {
            Some(refresh) => self.revoke_or_queue(refresh).await,
            None => false,
        };
        local.map(|()| settled)
    }
}

impl FsyncService {
    /// Sends a one-time code to `email`.
    pub fn login_start(&self, email: &str) -> SvcResult<()> {
        self.run(async { Ok(self.inner.api.otp_start(email).await?) })
    }

    /// Completes sign-in. The tokens are also delivered to `on_session_changed`.
    pub fn login_verify(&self, email: &str, code: &str) -> SvcResult<SessionTokens> {
        self.run(self.inner.login_verify(email, code))
    }

    /// Restores a session kept by the app (call before `ensure_account` / `start`).
    pub fn set_session(&self, tokens: SessionTokens) -> SvcResult<()> {
        self.rt_enter(|| self.inner.set_session(tokens))
    }

    /// Ends the session and forgets the pinned device list. The refresh token is revoked on the
    /// server: retried for a few seconds, then in the background while the service lives. `Ok`
    /// means signed out locally; if the backend stays unreachable and the app quits first, the
    /// token stays valid until its 30 day expiry.
    pub fn logout(&self) -> SvcResult<()> {
        self.logout_confirmed().map(drop)
    }

    /// `logout` that also reports whether the backend settled the revocation (`true`: revoked,
    /// already dead or refused for good). `false`: it is only queued in memory (backend
    /// unreachable, or no session was held), so the app should keep its refresh token and call
    /// `revoke_session` later.
    pub fn logout_confirmed(&self) -> SvcResult<bool> {
        self.run(self.inner.logout())
    }
}
