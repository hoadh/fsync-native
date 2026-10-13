//! fsync backend entrypoint.

use fsync_backend::{
    build_router, config::Config, db, email::sender::LogSender, presence::hub::InMemoryHub,
    state::AppState,
};
use std::{sync::Arc, time::Duration};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let cfg = Config::from_env()?;
    let pool = db::connect(&cfg.database_url).await?;
    db::migrate(&pool).await?;

    let (bind, header_timeout) = (cfg.bind_addr, cfg.header_read_timeout);
    let email = Arc::new(LogSender {
        dev_mode: cfg.dev_mode,
    });
    let state = AppState::new(pool.clone(), cfg, email, Arc::new(InMemoryHub::default()));

    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            if let Err(e) = db::purge_expired(&pool).await {
                tracing::warn!(kind = %fsync_backend::error::sqlx_kind(&e), "purge failed");
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "fsync-backend listening");
    fsync_backend::serve::serve(listener, build_router(state), header_timeout, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}
