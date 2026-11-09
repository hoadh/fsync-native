//! Answer of a job queued on the secrets thread.

use super::SecretsError;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

/// Pending answer of a queued job.
pub struct Reply<T>(pub(super) Receiver<Result<T, SecretsError>>);

impl<T> Reply<T> {
    pub fn wait(self, wait: Duration) -> Result<T, SecretsError> {
        match self.0.recv_timeout(wait) {
            Ok(r) => r,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(SecretsError::Timeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(SecretsError::ThreadGone),
        }
    }
}
