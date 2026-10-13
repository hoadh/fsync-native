//! `AuthAccount` / `AuthSession` extractors: validate the Bearer access token and that its
//! session (refresh family) is still live, so logout / revocation / account deletion cut off
//! outstanding access tokens immediately instead of at expiry.

use crate::{error::ApiError, state::AppState, util::now_ms};
use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};
use uuid::Uuid;

/// A step-up action (trust reset, account deletion) needs an OTP login at most this old.
pub const FRESH_AUTH_WINDOW_S: i64 = 5 * 60;

pub struct AuthAccount(pub Uuid);

/// Verified token claims for an existing account.
#[derive(Clone, Copy, Debug)]
pub struct AuthSession {
    pub account: Uuid,
    /// Refresh family id.
    pub sid: Uuid,
    pub auth_time: i64,
    /// Access token expiry, unix s.
    pub exp: i64,
}

impl AuthSession {
    /// Step-up check: the session's OTP login must be recent.
    pub fn require_fresh(&self) -> Result<(), ApiError> {
        if now_ms() / 1000 - self.auth_time <= FRESH_AUTH_WINDOW_S {
            Ok(())
        } else {
            Err(ApiError::ReauthRequired)
        }
    }
}

/// Verify a raw access token and confirm its session family is live (which implies the account
/// still exists: tokens cascade-delete with it). Indexed on `refresh_tokens(family_id)`.
pub async fn authenticate(state: &AppState, token: &str) -> Result<AuthSession, ApiError> {
    let c = state.tokens.verify_access(token)?;
    let (live,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM refresh_tokens
           WHERE family_id = $1 AND account_id = $2 AND revoked_at IS NULL AND expires_at > $3)",
    )
    .bind(c.sid)
    .bind(c.account)
    .bind(now_ms())
    .fetch_one(&state.db)
    .await?;
    if !live {
        return Err(ApiError::Unauthorized);
    }
    Ok(AuthSession {
        account: c.account,
        sid: c.sid,
        auth_time: c.auth_time,
        exp: c.exp,
    })
}

async fn from_parts(parts: &Parts, state: &AppState) -> Result<AuthSession, ApiError> {
    let token = parts
        .headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(ApiError::Unauthorized)?;
    authenticate(state, token).await
}

impl FromRequestParts<AppState> for AuthSession {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        from_parts(parts, state).await
    }
}

impl FromRequestParts<AppState> for AuthAccount {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        from_parts(parts, state).await.map(|s| Self(s.account))
    }
}
