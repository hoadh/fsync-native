//! Outbound email.
pub mod config;
pub mod sender;
pub mod smtp;

use std::sync::Arc;

/// Builds the sender the configuration asks for.
pub fn build_sender(
    mode: &config::EmailMode,
    dev_mode: bool,
) -> anyhow::Result<Arc<dyn sender::EmailSender>> {
    Ok(match mode {
        config::EmailMode::Log => Arc::new(sender::LogSender { dev_mode }),
        config::EmailMode::Smtp(c) => {
            tracing::info!(host = %c.host, port = c.port, "otp email via smtp");
            Arc::new(smtp::SmtpSender::new(c).map_err(|e| anyhow::anyhow!("{e}"))?)
        }
    })
}
