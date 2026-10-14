//! Email sender abstraction: `SmtpSender` (smtp.rs) delivers; `LogSender` is the dev sink.

use async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
#[error("email delivery failed: {msg}")]
pub struct EmailError {
    pub msg: String,
    /// The provider may have accepted the mail despite the error (a timeout after the message
    /// was handed over). Callers must then treat the send as spent and the code as live.
    pub maybe_delivered: bool,
}

impl EmailError {
    /// A failure that certainly delivered nothing (refused, TLS/auth failure, rejected reply).
    pub fn failed(msg: impl Into<String>) -> Self {
        Self {
            msg: msg.into(),
            maybe_delivered: false,
        }
    }

    pub fn ambiguous(msg: impl Into<String>) -> Self {
        Self {
            msg: msg.into(),
            maybe_delivered: true,
        }
    }
}

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
