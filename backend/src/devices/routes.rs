//! GET/PUT /v1/device-list and GET /v1/device-list/chain.

use super::{
    repo,
    verify::{Decision, decide},
};
use crate::{
    auth::extractor::{AuthAccount, AuthSession},
    error::{ApiError, ApiResult},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Query, State},
    http::HeaderMap,
    routing::get,
};
use fsync_proto::api_dto::{ChainResponse, DeviceListResponse, PutDeviceList, encode_signed};
use serde::Deserialize;
use std::time::Duration;

const CHAIN_LIMIT: i64 = 100;
const PUTS_PER_HOUR: usize = 30;
const RESETS_PER_DAY: usize = 3;
const DAY: Duration = Duration::from_secs(86_400);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/device-list", get(get_list).put(put_list))
        .route("/device-list/chain", get(get_chain))
}

fn response(signed: &fsync_proto::SignedDeviceList) -> ApiResult<Json<DeviceListResponse>> {
    DeviceListResponse::new(signed)
        .map(Json)
        .map_err(ApiError::internal)
}

async fn get_list(
    State(s): State<AppState>,
    AuthAccount(account): AuthAccount,
) -> ApiResult<Json<DeviceListResponse>> {
    let stored = repo::get(&s.db, account).await?.ok_or(ApiError::NotFound)?;
    response(&stored.signed)
}

/// `If-Match`: seq of the stored list, or absent/`none` when no list exists yet.
/// Skipped for a trust reset (`X-Fsync-Trust-Reset: 1`), which replaces the chain.
fn check_if_match(headers: &HeaderMap, stored_seq: Option<i64>) -> ApiResult<()> {
    let given = match headers.get("if-match").map(|v| v.to_str()) {
        None => None,
        Some(Ok("none")) => None,
        Some(Ok(v)) => Some(
            v.trim_matches('"')
                .parse::<i64>()
                .map_err(|_| ApiError::BadRequest("If-Match must be a seq or 'none'".into()))?,
        ),
        Some(Err(_)) => return Err(ApiError::BadRequest("bad If-Match".into())),
    };
    if given == stored_seq {
        Ok(())
    } else {
        Err(ApiError::Conflict(format!(
            "If-Match does not match stored seq ({stored_seq:?})"
        )))
    }
}

/// A trust reset replaces the chain root, so it needs a fresh OTP login (<= 5 min), is capped at
/// 3/day/account, and (like every successful PUT) is broadcast to the account's sockets.
async fn put_list(
    State(s): State<AppState>,
    session: AuthSession,
    headers: HeaderMap,
    Json(body): Json<PutDeviceList>,
) -> ApiResult<Json<DeviceListResponse>> {
    let account = session.account;
    let reset_key = format!("devlist:reset:{account}");
    let reset = headers
        .get("x-fsync-trust-reset")
        .is_some_and(|v| v.as_bytes() == b"1");
    if reset {
        session.require_fresh()?;
        // Only successful resets consume the daily budget (recorded after commit).
        s.limiter.peek(&reset_key, RESETS_PER_DAY, DAY)?;
    }
    s.limiter.check(
        &format!("devlist:put:{account}"),
        PUTS_PER_HOUR,
        Duration::from_secs(3600),
    )?;
    let new = body
        .decode()
        .map_err(|e| ApiError::BadRequest(format!("undecodable device list: {e}")))?;

    let mut tx = s.db.begin().await?;
    let stored = repo::get_for_update(&mut tx, account).await?;
    // Idempotent retry of the already-stored list wins over any If-Match mismatch.
    if let Some(st) = stored.as_ref().filter(|st| st.signed == new) {
        return response(&st.signed);
    }
    if !reset {
        check_if_match(&headers, stored.as_ref().map(|x| x.seq))?;
    }
    let decision = decide(account, stored.as_ref().map(|x| &x.signed), &new, reset)?;
    let (epoch, list) = match decision {
        Decision::Unchanged => return response(&new),
        Decision::Genesis(l) => (0, l),
        Decision::Next(l) => (stored.as_ref().map_or(0, |x| x.epoch), l),
        Decision::Reset(l) => (stored.as_ref().map_or(0, |x| x.epoch) + 1, l),
    };
    let seq = i64::try_from(list.seq).map_err(ApiError::internal)?;
    if !repo::store(&mut tx, account, epoch, seq, &new).await? {
        return Err(ApiError::Conflict("concurrent device-list update".into()));
    }
    tx.commit().await?;
    if reset {
        s.limiter.record(&reset_key, DAY);
    }
    let listed: Vec<_> = list.devices.iter().map(|d| d.node_id).collect();
    s.hub.device_list_changed(account, list.seq, &listed);
    response(&new)
}

#[derive(Deserialize)]
struct ChainQuery {
    from_seq: Option<i64>,
}

async fn get_chain(
    State(s): State<AppState>,
    AuthAccount(account): AuthAccount,
    Query(q): Query<ChainQuery>,
) -> ApiResult<Json<ChainResponse>> {
    let stored = repo::get(&s.db, account).await?.ok_or(ApiError::NotFound)?;
    let from = q.from_seq.unwrap_or(0).max(0);
    let found = repo::chain(&s.db, account, stored.epoch, from, CHAIN_LIMIT).await?;
    let chain = found
        .lists
        .iter()
        .map(|l| encode_signed(l).map_err(ApiError::internal))
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(Json(ChainResponse {
        chain,
        earliest_seq: u64::try_from(found.earliest_seq).map_err(ApiError::internal)?,
        truncated: found.truncated,
    }))
}
