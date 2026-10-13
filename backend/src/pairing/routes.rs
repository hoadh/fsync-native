//! POST /v1/pairing, GET /v1/pairing/{code}.

use super::repo;
use crate::{
    auth::extractor::AuthAccount,
    error::{ApiError, ApiResult},
    state::AppState,
    util::{now_ms, random_digits6, relay_url_ok},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use fsync_proto::{
    NodeId,
    api_dto::{PairingCreate, PairingCreated, PairingLookup, is_valid_pairing_code},
};
use std::time::Duration;

const HOUR: Duration = Duration::from_secs(3600);
const PER_ACCOUNT_PER_HOUR: usize = 30;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/pairing", post(create))
        .route("/pairing/{code}", get(lookup))
}

fn validate_relay(url: &Option<String>, dev_mode: bool) -> ApiResult<()> {
    match url {
        Some(u) if !relay_url_ok(u, dev_mode) => Err(ApiError::BadRequest(
            "relay_url must be a short https URL".into(),
        )),
        _ => Ok(()),
    }
}

async fn create(
    State(s): State<AppState>,
    AuthAccount(account): AuthAccount,
    Json(body): Json<PairingCreate>,
) -> ApiResult<Json<PairingCreated>> {
    validate_relay(&body.relay_url, s.cfg.dev_mode)?;
    s.limiter.check(
        &format!("pair:create:{account}"),
        PER_ACCOUNT_PER_HOUR,
        HOUR,
    )?;
    let expires_at = now_ms() + repo::CODE_TTL_MS;
    for _ in 0..8 {
        let code = random_digits6()?;
        let hash = repo::code_hash(&s.hash_key, account, &code);
        let inserted = repo::try_insert(
            &s.db,
            &hash,
            account,
            &body.node_id,
            body.relay_url.as_deref(),
            expires_at,
        )
        .await?;
        if inserted {
            return Ok(Json(PairingCreated {
                code,
                expires_at: u64::try_from(expires_at).map_err(ApiError::internal)?,
            }));
        }
    }
    Err(ApiError::Internal("could not allocate pairing code".into()))
}

async fn lookup(
    State(s): State<AppState>,
    AuthAccount(account): AuthAccount,
    Path(code): Path<String>,
) -> ApiResult<Json<PairingLookup>> {
    // Counts every lookup (hit or miss): bounds online guessing of the 6-digit space.
    s.limiter.check(
        &format!("pair:lookup:{account}"),
        PER_ACCOUNT_PER_HOUR,
        HOUR,
    )?;
    if !is_valid_pairing_code(&code) {
        return Err(ApiError::BadRequest("code must be 6 digits".into()));
    }
    let hash = repo::code_hash(&s.hash_key, account, &code);
    let (node, relay_url) = repo::consume(&s.db, &hash, account)
        .await?
        .ok_or(ApiError::NotFound)?;
    let node_id: NodeId = node.parse().map_err(ApiError::internal)?;
    Ok(Json(PairingLookup { node_id, relay_url }))
}
