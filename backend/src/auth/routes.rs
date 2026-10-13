//! /v1/auth/* and DELETE /v1/account.

use super::{extractor::AuthSession, otp, refresh};
use crate::{
    client_ip::client_key,
    error::{ApiError, ApiResult},
    state::AppState,
    util::now_ms,
};
use axum::{
    Json, Router,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    routing::{delete, post},
};
use fsync_proto::api_dto::{OtpStart, OtpVerify, OtpVerifyResponse, Refresh, TokenPair};
use std::net::SocketAddr;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/otp/start", post(otp_start))
        .route("/auth/otp/verify", post(otp_verify))
        .route("/auth/refresh", post(refresh_tokens))
        .route("/auth/logout", post(logout))
        .route("/account", delete(delete_account))
}

async fn otp_start(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<OtpStart>,
) -> ApiResult<StatusCode> {
    let ip = client_key(&s.cfg, &headers, peer);
    otp::start(&s, &body.email, &ip).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn otp_verify(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<OtpVerify>,
) -> ApiResult<Json<OtpVerifyResponse>> {
    let ip = client_key(&s.cfg, &headers, peer);
    let account = otp::verify(&s, &body.email, &body.code, &ip).await?;
    let (family, auth_time_ms) = (Uuid::new_v4(), now_ms());
    let mut conn = s.db.acquire().await?;
    let refresh = refresh::issue(&mut conn, account, family, auth_time_ms).await?;
    drop(conn);
    refresh::cap_families(&s.db, &*s.hub, account).await?;
    Ok(Json(OtpVerifyResponse {
        tokens: TokenPair {
            access: s.tokens.issue_access(account, family, auth_time_ms)?,
            refresh,
        },
        account_id: account,
    }))
}

async fn refresh_tokens(
    State(s): State<AppState>,
    Json(body): Json<Refresh>,
) -> ApiResult<Json<TokenPair>> {
    let r = refresh::rotate(&s.db, &*s.hub, &body.refresh).await?;
    Ok(Json(TokenPair {
        access: s.tokens.issue_access(r.account, r.family, r.auth_time_ms)?,
        refresh: r.token,
    }))
}

async fn logout(State(s): State<AppState>, Json(body): Json<Refresh>) -> ApiResult<StatusCode> {
    refresh::revoke(&s.db, &*s.hub, &body.refresh).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GDPR erasure: cascades to tokens, device lists, history, pairing codes.
/// Step-up: needs an OTP login within the last 5 minutes (`reauth_required` otherwise).
async fn delete_account(State(s): State<AppState>, session: AuthSession) -> ApiResult<StatusCode> {
    session.require_fresh()?;
    let account = session.account;
    let mut tx = s.db.begin().await?;
    let email: Option<(String,)> =
        sqlx::query_as("DELETE FROM accounts WHERE id = $1 RETURNING email")
            .bind(account)
            .fetch_optional(&mut *tx)
            .await?;
    let (email,) = email.ok_or(ApiError::Unauthorized)?;
    sqlx::query("DELETE FROM otp_codes WHERE email = $1")
        .bind(email)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    s.hub.close_account(account);
    Ok(StatusCode::NO_CONTENT)
}
