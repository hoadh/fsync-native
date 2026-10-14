//! Global OTP send budget over a rolling 24 h window, kept in Postgres so it survives restarts and
//! holds across instances. Protects the SMTP provider's daily recipient limit and bounds how much
//! mail an attacker spread over many IPs and addresses can trigger.

use crate::{
    error::{ApiError, ApiResult},
    util::now_ms,
};
use sqlx::PgPool;

const WINDOW_MS: i64 = 24 * 3600 * 1000;
/// Serializes reservations so concurrent starts cannot overshoot the cap.
const LOCK_KEY: i64 = 0x6673_796e_635f_6f74;

/// Reserve one send slot; `RateLimited` (429) when `cap` sends happened in the last 24 h.
/// Returns the slot id for `release` if the send then fails.
pub async fn reserve(db: &PgPool, cap: u32) -> ApiResult<i64> {
    let now = now_ms();
    let cutoff = now - WINDOW_MS;
    let mut tx = db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(LOCK_KEY)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM otp_send_log WHERE sent_at <= $1")
        .bind(cutoff)
        .execute(&mut *tx)
        .await?;
    let (used, oldest): (i64, Option<i64>) =
        sqlx::query_as("SELECT count(*), min(sent_at) FROM otp_send_log")
            .fetch_one(&mut *tx)
            .await?;
    if used >= i64::from(cap) {
        let wait_ms = oldest.map_or(WINDOW_MS, |o| o + WINDOW_MS - now);
        tracing::warn!(cap, "global OTP send cap reached; start refused");
        return Err(ApiError::RateLimited {
            retry_after_s: u64::try_from(wait_ms / 1000).unwrap_or(0).max(1),
        });
    }
    let (id,): (i64,) =
        sqlx::query_as("INSERT INTO otp_send_log (sent_at) VALUES ($1) RETURNING id")
            .bind(now)
            .fetch_one(&mut *tx)
            .await?;
    tx.commit().await?;
    Ok(id)
}

/// True when `cap` sends happened in the last 24 h, i.e. new OTP starts are being refused.
/// Read-only (no lock, no cleanup); feeds `/health/otp` so an external probe can alert on it.
pub async fn reached(db: &PgPool, cap: u32) -> ApiResult<bool> {
    let cutoff = now_ms() - WINDOW_MS;
    let used: i64 = sqlx::query_scalar("SELECT count(*) FROM otp_send_log WHERE sent_at > $1")
        .bind(cutoff)
        .fetch_one(db)
        .await?;
    Ok(used >= i64::from(cap))
}

/// Give the slot back after a failed send (nothing was delivered, so it is not spent).
pub async fn release(db: &PgPool, id: i64) {
    if let Err(e) = sqlx::query("DELETE FROM otp_send_log WHERE id = $1")
        .bind(id)
        .execute(db)
        .await
    {
        tracing::warn!(kind = %crate::error::sqlx_kind(&e), "otp send slot release failed");
    }
}
