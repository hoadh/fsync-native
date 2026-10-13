//! Opaque rotating refresh tokens with reuse detection (family revocation).
//! Rotation consumes a token. Reuse < 10 s after rotation is a benign race (409
//! `refresh_in_progress`); older reuse revokes the family. Families live 90 d; max 20 per account.

use crate::{
    error::{ApiError, ApiResult},
    presence::hub::PresenceHub,
    util::{now_ms, random_bytes},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use sqlx::{PgConnection, PgPool};
use std::collections::HashSet;
use uuid::Uuid;

pub const REFRESH_TTL_MS: i64 = 30 * 24 * 3600 * 1000;
pub const FAMILY_TTL_MS: i64 = 90 * 24 * 3600 * 1000;
pub const ROTATION_GRACE_MS: i64 = 10_000;
pub const MAX_FAMILIES: i64 = 20;

pub struct Rotated {
    pub account: Uuid,
    pub family: Uuid,
    pub auth_time_ms: i64,
    pub token: String,
}

fn hash(token: &str) -> [u8; 32] {
    *blake3::hash(token.as_bytes()).as_bytes()
}

/// Insert a new refresh token for `family` and return the plaintext token.
pub async fn issue(
    conn: &mut PgConnection,
    account: Uuid,
    family: Uuid,
    auth_time_ms: i64,
) -> ApiResult<String> {
    let token = B64.encode(random_bytes::<32>()?);
    let now = now_ms();
    let expires = (now + REFRESH_TTL_MS).min(auth_time_ms.saturating_add(FAMILY_TTL_MS));
    sqlx::query(
        "INSERT INTO refresh_tokens
           (account_id, family_id, token_hash, expires_at, created_at, auth_time)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(account)
    .bind(family)
    .bind(hash(&token).as_slice())
    .bind(expires)
    .bind(now)
    .bind(auth_time_ms)
    .execute(conn)
    .await?;
    Ok(token)
}

pub async fn rotate(db: &PgPool, hub: &dyn PresenceHub, token: &str) -> ApiResult<Rotated> {
    let h = hash(token);
    let now = now_ms();
    let mut tx = db.begin().await?;
    let row: Option<(Uuid, Uuid, i64)> = sqlx::query_as(
        "UPDATE refresh_tokens SET revoked_at = $2, rotated_at = $2
         WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > $2 AND auth_time > $3
         RETURNING account_id, family_id, auth_time",
    )
    .bind(h.as_slice())
    .bind(now)
    .bind(now - FAMILY_TTL_MS)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((account, family, auth_time_ms)) = row else {
        drop(tx);
        return Err(reject(db, hub, &h, now).await);
    };
    let token = issue(&mut tx, account, family, auth_time_ms).await?;
    tx.commit().await?;
    Ok(Rotated {
        account,
        family,
        auth_time_ms,
        token,
    })
}

/// Classify a failed rotation: benign retry (409), theft (revoke family, 401) or unknown (401).
async fn reject(db: &PgPool, hub: &dyn PresenceHub, h: &[u8; 32], now: i64) -> ApiError {
    match classify(db, hub, h, now).await {
        Ok(e) | Err(e) => e,
    }
}

async fn classify(
    db: &PgPool,
    hub: &dyn PresenceHub,
    h: &[u8; 32],
    now: i64,
) -> ApiResult<ApiError> {
    let row: Option<(Option<i64>, Uuid)> =
        sqlx::query_as("SELECT rotated_at, family_id FROM refresh_tokens WHERE token_hash = $1")
            .bind(h.as_slice())
            .fetch_optional(db)
            .await?;
    let Some((rotated_at, family)) = row else {
        return Ok(ApiError::Unauthorized);
    };
    if rotated_at.is_some_and(|t| now - t < ROTATION_GRACE_MS) {
        let (alive,): (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM refresh_tokens
               WHERE family_id = $1 AND revoked_at IS NULL AND expires_at > $2)",
        )
        .bind(family)
        .bind(now)
        .fetch_one(db)
        .await?;
        return Ok(if alive {
            ApiError::ConflictCode {
                code: "refresh_in_progress",
                message: "token was just rotated; retry with the newer token".into(),
            }
        } else {
            ApiError::Unauthorized
        });
    }
    let revoked = revoke_families(
        db,
        "AND family_id IN (SELECT family_id FROM refresh_tokens
                           WHERE token_hash = $1 AND revoked_at IS NOT NULL)",
        h,
    )
    .await?;
    if !revoked.is_empty() {
        tracing::warn!("refresh token reuse detected: family revoked");
    }
    close_sessions(hub, &revoked);
    Ok(ApiError::Unauthorized)
}

/// Logout: revoke the token's whole session family.
pub async fn revoke(db: &PgPool, hub: &dyn PresenceHub, token: &str) -> ApiResult<()> {
    let revoked = revoke_families(
        db,
        "AND family_id IN (SELECT family_id FROM refresh_tokens WHERE token_hash = $1)",
        &hash(token),
    )
    .await?;
    close_sessions(hub, &revoked);
    Ok(())
}

/// Keep only the `MAX_FAMILIES` newest live families of the account; revoke the rest.
pub async fn cap_families(db: &PgPool, hub: &dyn PresenceHub, account: Uuid) -> ApiResult<()> {
    let now = now_ms();
    let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "UPDATE refresh_tokens SET revoked_at = $2
         WHERE account_id = $1 AND revoked_at IS NULL AND family_id IN (
           SELECT family_id FROM refresh_tokens
           WHERE account_id = $1 AND revoked_at IS NULL AND expires_at > $2
           GROUP BY family_id ORDER BY min(auth_time) DESC, family_id OFFSET $3)
         RETURNING account_id, family_id",
    )
    .bind(account)
    .bind(now)
    .bind(MAX_FAMILIES)
    .fetch_all(db)
    .await?;
    close_sessions(hub, &dedup(rows));
    Ok(())
}

fn dedup(rows: Vec<(Uuid, Uuid)>) -> Vec<(Uuid, Uuid)> {
    rows.into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

fn close_sessions(hub: &dyn PresenceHub, revoked: &[(Uuid, Uuid)]) {
    for (account, family) in revoked {
        hub.close_session(*account, *family);
    }
}

/// `filter` is a static SQL fragment (never user input) selecting families via `$1`.
async fn revoke_families(
    db: &PgPool,
    filter: &'static str,
    h: &[u8; 32],
) -> ApiResult<Vec<(Uuid, Uuid)>> {
    let sql = format!(
        "UPDATE refresh_tokens SET revoked_at = $2 WHERE revoked_at IS NULL {filter}
         RETURNING account_id, family_id"
    );
    let rows = sqlx::query_as(&sql)
        .bind(h.as_slice())
        .bind(now_ms())
        .fetch_all(db)
        .await?;
    Ok(dedup(rows))
}
