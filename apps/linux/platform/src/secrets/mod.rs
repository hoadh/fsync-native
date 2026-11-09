//! Persistent secrets: device secret, history key, session tokens, email, pending revoke.
//!
//! Backend choice is made once per data dir and recorded in `secrets-backend` (see
//! `backend_marker`). Later runs follow the marker only: an unavailable/locked Secret Service
//! means `WaitingForKeyring`, never a fallback and never new keys.

pub mod backend_marker;
pub mod bootstrap;
pub mod file_fallback;
mod model;
mod reply;
pub mod secret_service;
pub mod secrets_thread;
mod unlock_policy;
mod worker;

pub use backend_marker::{Backend, may_create_keys};
pub use bootstrap::load_or_create;
pub use model::{Secrets, SessionTokens};
pub use reply::Reply;
pub use secret_service::SystemProviders;
pub use secrets_thread::{SecretsHandle, SecretsStatus};

use std::path::Path;
use std::time::Duration;

/// How long `org.freedesktop.secrets` gets to answer on the first run.
pub const FIRST_RUN_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, thiserror::Error)]
pub enum SecretsError {
    #[error("secret service unavailable or locked: {0}")]
    Unavailable(String),
    #[error("the stored keys are missing")]
    KeysMissing,
    #[error("waiting for the keyring")]
    WaitingForKeyring,
    #[error("secrets operation timed out")]
    Timeout,
    #[error("insecure secrets file: {0}")]
    Insecure(String),
    #[error("invalid secrets data: {0}")]
    Format(String),
    #[error("secrets thread is gone")]
    ThreadGone,
    #[error("secrets io error: {0}")]
    Io(#[from] std::io::Error),
}

/// One storage backend. Blocking; only called from the `fsync-secrets` thread.
pub trait SecretStore: Send {
    fn load(&mut self) -> Result<Option<Secrets>, SecretsError>;
    fn save(&mut self, secrets: &Secrets) -> Result<(), SecretsError>;
    fn clear(&mut self) -> Result<(), SecretsError>;
}

/// Seam over the session bus so tests never touch the user's real keyring.
pub trait Providers: Send + Sync {
    /// Does `org.freedesktop.secrets` answer within `timeout`?
    fn secret_service_answers(&self, timeout: Duration) -> bool;
    fn open_secret_service(&self, data_dir: &Path) -> Result<Box<dyn SecretStore>, SecretsError>;
    /// The user asked to unlock the keyring: allow one more unlock prompt.
    fn request_unlock(&self) {}
}

/// `FSYNC_SECRETS=file` forces the file backend; honored only in debug or `e2e-control` builds.
pub fn file_override_allowed() -> bool {
    cfg!(debug_assertions) || cfg!(feature = "e2e-control")
}

/// Result of choosing/opening a backend.
pub struct Opened {
    /// None while waiting for the keyring.
    pub store: Option<Box<dyn SecretStore>>,
    pub backend: Option<Backend>,
    /// First run without a Secret Service: caller shows the persistent "weaker storage" banner.
    pub file_fallback_warning: bool,
}

/// Choose and open the backend for `data_dir`. `force_file` must already be gated by
/// `file_override_allowed()`.
pub fn open_store(
    data_dir: &Path,
    providers: &dyn Providers,
    force_file: bool,
) -> Result<Opened, SecretsError> {
    let file = |warn: bool| -> Result<Opened, SecretsError> {
        Ok(Opened {
            store: Some(Box::new(file_fallback::FileStore::new(data_dir))),
            backend: Some(Backend::File),
            file_fallback_warning: warn,
        })
    };
    let marker = backend_marker::read_marker(data_dir)?;
    if force_file {
        if marker.is_none() {
            backend_marker::write_marker(data_dir, Backend::File)?;
        }
        return file(false);
    }
    match marker {
        Some(Backend::File) => file(false),
        Some(Backend::SecretService) => Ok(match providers.open_secret_service(data_dir) {
            Ok(store) => Opened {
                store: Some(store),
                backend: Some(Backend::SecretService),
                file_fallback_warning: false,
            },
            Err(e) => {
                tracing::warn!("secret service not usable, waiting for keyring: {e}");
                Opened {
                    store: None,
                    backend: Some(Backend::SecretService),
                    file_fallback_warning: false,
                }
            }
        }),
        None if providers.secret_service_answers(FIRST_RUN_PROBE_TIMEOUT) => {
            match providers.open_secret_service(data_dir) {
                Ok(store) => {
                    backend_marker::write_marker(data_dir, Backend::SecretService)?;
                    Ok(Opened {
                        store: Some(store),
                        backend: Some(Backend::SecretService),
                        file_fallback_warning: false,
                    })
                }
                // Answers but unusable (locked/prompt pending): nothing persisted yet, so wait
                // instead of committing this data dir to the weaker backend.
                Err(e) => {
                    tracing::warn!("secret service answered but is not usable yet: {e}");
                    Ok(Opened {
                        store: None,
                        backend: None,
                        file_fallback_warning: false,
                    })
                }
            }
        }
        None => {
            backend_marker::write_marker(data_dir, Backend::File)?;
            file(true)
        }
    }
}

#[cfg(test)]
#[path = "tests_open_store.rs"]
mod tests_open_store;
