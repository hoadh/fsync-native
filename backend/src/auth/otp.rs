//! OTP issue/verify. Codes are keyed-hashed, expire in 10 min, allow 5 attempts.
//! A repeated `start` does not replace a live code younger than `RESEND_AFTER_MS` (a third party
//! cannot keep invalidating a victim's code); an older live code may be replaced so a lost
//! email can be re-requested. Only starts that issue a code count toward the per-email limit.

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
    util::{hash_from_slice, keyed_hash, now_ms, random_digits6},
};
use std::time::Duration;
use uuid::Uuid;

pub const OTP_TTL_MS: i64 = 10 * 60 * 1000;
pub const MAX_ATTEMPTS: i32 = 5;
/// A live code at least this old is replaced by a new `start`.
pub const RESEND_AFTER_MS: i64 = 60 * 1000;
const HOUR: Duration = Duration::from_secs(3600);
const START_PER_EMAIL: usize = 5;
const START_PER_IP: usize = 20;
const VERIFY_PER_IP: usize = 30;

/// Trim + lowercase + minimal shape check.
pub fn normalize_email(raw: &str) -> ApiResult<String> {
    let e = raw.trim().to_lowercase();
    let ok = e.len() <= 254
        && !e.chars().any(|c| c.is_whitespace() || c.is_control())
        && e.split_once('@')
            .is_some_and(|(l, d)| !l.is_empty() && d.contains('.') && !d.contains('@'));
    if ok {
        Ok(e)
    } else {
        Err(ApiError::BadRequest("invalid email".into()))
    }
}

pub async fn start(state: &AppState, email: &str, ip: &str) -> ApiResult<()> {
    let email = normalize_email(email)?;
    state
        .limiter
        .check(&format!("otp:ip:{ip}"), START_PER_IP, HOUR)?;
    let email_key = format!("otp:email:{email}");
    // Peek first so an exhausted email cannot be used to replace (and so kill) a live code.
    state.limiter.peek(&email_key, START_PER_EMAIL, HOUR)?;
    let code = random_digits6()?;
    let hash = keyed_hash(&state.hash_key, "otp", &[email.as_bytes(), code.as_bytes()]);
    let hash = hash.as_bytes().as_slice();
    let now = now_ms();
    // created_at = expires_at - TTL, so "older than RESEND_AFTER_MS" is a bound on expires_at.
    let created = sqlx::query(
        "INSERT INTO otp_codes (email, code_hash, expires_at, attempts) VALUES ($1, $2, $3, 0)
         ON CONFLICT (email) DO UPDATE
           SET code_hash = EXCLUDED.code_hash, expires_at = EXCLUDED.expires_at, attempts = 0
         WHERE otp_codes.expires_at <= $4 OR otp_codes.attempts >= $5
            OR otp_codes.expires_at <= $6",
    )
    .bind(&email)
    .bind(hash)
    .bind(now + OTP_TTL_MS)
    .bind(now)
    .bind(MAX_ATTEMPTS)
    .bind(now + OTP_TTL_MS - RESEND_AFTER_MS)
    .execute(&state.db)
    .await?
    .rows_affected();
    if created == 0 {
        // A fresh live code is pending; only its hash is stored so it cannot be re-sent.
        return Ok(());
    }
    state.limiter.record(&email_key, HOUR);
    if let Err(e) = state.email.send_otp(&email, &code).await {
        // Never leave a live code the user was not told about.
        if let Err(db) = sqlx::query("DELETE FROM otp_codes WHERE email = $1 AND code_hash = $2")
            .bind(&email)
            .bind(hash)
            .execute(&state.db)
            .await
        {
            tracing::warn!(kind = %crate::error::sqlx_kind(&db), "otp cleanup after send failure failed");
        }
        return Err(ApiError::internal(e));
    }
    Ok(())
}

/// Returns the account id (created on first successful login).
pub async fn verify(state: &AppState, email: &str, code: &str, ip: &str) -> ApiResult<Uuid> {
    state
        .limiter
        .check(&format!("otp:verify:ip:{ip}"), VERIFY_PER_IP, HOUR)?;
    let email = normalize_email(email).map_err(|_| ApiError::Unauthorized)?;
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::Unauthorized);
    }
    // Every guess burns an attempt, whether or not it is right.
    let row: Option<(Vec<u8>,)> = sqlx::query_as(
        "UPDATE otp_codes SET attempts = attempts + 1
         WHERE email = $1 AND expires_at > $2 AND attempts < $3
         RETURNING code_hash",
    )
    .bind(&email)
    .bind(now_ms())
    .bind(MAX_ATTEMPTS)
    .fetch_optional(&state.db)
    .await?;
    // Hash and compare even when no row matched, so timing does not reveal a pending code.
    let expected = keyed_hash(&state.hash_key, "otp", &[email.as_bytes(), code.as_bytes()]);
    let (stored, found) = match row {
        Some((s,)) => (s, true),
        None => (vec![0; 32], false),
    };
    // blake3::Hash equality is constant-time.
    let matches = hash_from_slice(&stored) == Some(expected);
    if !(found && matches) {
        return Err(ApiError::Unauthorized);
    }
    // Single use: only one concurrent verifier can delete the row.
    let used = sqlx::query("DELETE FROM otp_codes WHERE email = $1 AND code_hash = $2")
        .bind(&email)
        .bind(stored.as_slice())
        .execute(&state.db)
        .await?
        .rows_affected();
    if used != 1 {
        return Err(ApiError::Unauthorized);
    }
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO accounts (email, created_at) VALUES ($1, $2)
         ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email RETURNING id",
    )
    .bind(&email)
    .bind(now_ms())
    .fetch_one(&state.db)
    .await?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::normalize_email;

    #[test]
    fn email_validation() {
        assert_eq!(normalize_email(" A@B.co ").unwrap(), "a@b.co");
        for bad in ["", "a", "a@b", "@b.co", "a@@b.co", "a b@c.de", "a@b.c@d"] {
            assert!(normalize_email(bad).is_err(), "{bad}");
        }
    }
}
