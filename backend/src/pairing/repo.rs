//! Postgres access for pairing codes. Codes are stored as keyed hashes bound to the account.

use crate::{
    error::ApiResult,
    util::{keyed_hash, now_ms},
};
use fsync_proto::NodeId;
use sqlx::PgPool;
use uuid::Uuid;

pub const CODE_TTL_MS: i64 = 5 * 60 * 1000;

pub fn code_hash(key: &[u8; 32], account: Uuid, code: &str) -> [u8; 32] {
    *keyed_hash(key, "pairing", &[account.as_bytes(), code.as_bytes()]).as_bytes()
}

/// Try to claim `hash`; false if an active (unexpired, unused) offer already holds it.
pub async fn try_insert(
    db: &PgPool,
    hash: &[u8; 32],
    account: Uuid,
    node: &NodeId,
    relay_url: Option<&str>,
    expires_at: i64,
) -> ApiResult<bool> {
    let n = sqlx::query(
        "INSERT INTO pairing_codes (code_hash, account_id, node_id, relay_url, expires_at)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (code_hash) DO UPDATE
           SET node_id = EXCLUDED.node_id, relay_url = EXCLUDED.relay_url,
               expires_at = EXCLUDED.expires_at, used_at = NULL
         WHERE pairing_codes.expires_at < $6 OR pairing_codes.used_at IS NOT NULL",
    )
    .bind(hash.as_slice())
    .bind(account)
    .bind(node.to_string())
    .bind(relay_url)
    .bind(expires_at)
    .bind(now_ms())
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// Consume the code (single use). `None` if unknown, expired, used, or another account's.
pub async fn consume(
    db: &PgPool,
    hash: &[u8; 32],
    account: Uuid,
) -> ApiResult<Option<(String, Option<String>)>> {
    let now = now_ms();
    let row = sqlx::query_as(
        "UPDATE pairing_codes SET used_at = $3
         WHERE code_hash = $1 AND account_id = $2 AND used_at IS NULL AND expires_at > $3
         RETURNING node_id, relay_url",
    )
    .bind(hash.as_slice())
    .bind(account)
    .bind(now)
    .fetch_optional(db)
    .await?;
    Ok(row)
}

/// A live offer (unexpired; used or not) lets that node connect to presence before it is listed.
pub async fn has_live_offer(db: &PgPool, account: Uuid, node: &NodeId) -> ApiResult<bool> {
    let row: Option<(i32,)> = sqlx::query_as(
        "SELECT 1 FROM pairing_codes WHERE account_id = $1 AND node_id = $2 AND expires_at > $3 LIMIT 1",
    )
    .bind(account)
    .bind(node.to_string())
    .bind(now_ms())
    .fetch_optional(db)
    .await?;
    Ok(row.is_some())
}
