//! Email sender abstraction. No real provider is wired yet (provider choice unresolved);
//! `LogSender` is the dev sink.

use async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
#[error("email delivery failed: {0}")]
pub struct EmailError(pub String);

#[async_trait]
pub trait EmailSender: Send + Sync {
    async fn send_otp(&self, to: &str, code: &str) -> Result<(), EmailError>;
}

/// Logs instead of sending. The code itself is only logged at debug level in dev mode.
pub struct LogSender {
    pub dev_mode: bool,
}

#[async_trait]
impl EmailSender for LogSender {
    async fn send_otp(&self, to: &str, code: &str) -> Result<(), EmailError> {
        let tag = &blake3::hash(to.as_bytes()).to_hex()[..12];
        tracing::info!(recipient = tag, "otp email (log sink, not delivered)");
        if self.dev_mode {
            tracing::debug!(to, code, "dev otp");
        }
        Ok(())
    }
}
