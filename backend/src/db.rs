//! Postgres pool, migrations, periodic cleanup.

use crate::util::now_ms;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;

pub async fn connect(url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(5))
        .connect(url)
        .await?;
    Ok(pool)
}

pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

/// Delete expired OTPs, pairing codes and long-dead refresh tokens.
pub async fn purge_expired(pool: &PgPool) -> Result<(), sqlx::Error> {
    let now = now_ms();
    sqlx::query("DELETE FROM otp_codes WHERE expires_at < $1")
        .bind(now)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM pairing_codes WHERE expires_at < $1")
        .bind(now)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM refresh_tokens WHERE expires_at < $1")
        .bind(now)
        .execute(pool)
        .await?;
    Ok(())
}
