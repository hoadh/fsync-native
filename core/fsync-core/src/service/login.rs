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

    pub(super) async fn logout(self: &Arc<Self>) -> SvcResult<()> {
        // Local sign-out happens even when the server cannot be reached.
        let remote = self.api.logout().await;
        self.set_account(None)?;
        lock(&self.live)?.presence = None;
        self.trust.clear()?;
        remote.map_err(Into::into)
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

    /// Ends the session, forgets the pinned device list.
    pub fn logout(&self) -> SvcResult<()> {
        self.run(self.inner.logout())
    }
}
