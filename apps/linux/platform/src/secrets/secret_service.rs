//! Secret Service (GNOME Keyring / KWallet / KeePassXC) through oo7's D-Bus API. oo7 is async, so
//! the store owns a private current-thread runtime and is only ever used from the secrets thread.
//! `oo7::Keyring` is not used: it silently switches to a sandbox file backend.

use super::unlock_policy::UnlockPolicy;
use super::{Providers, SecretStore, Secrets, SecretsError};
use crate::APP_ID;
use oo7::dbus::{Collection, Service};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use zeroize::Zeroizing;

/// Bound for one D-Bus operation that does not wait for the user.
const OP_TIMEOUT: Duration = Duration::from_secs(5);
/// Bound for an unlock prompt: the user needs time to type the password. Dropping oo7's
/// `unlock()` future does NOT dismiss the prompt (oo7 has no cancel-on-drop; `Dismiss` is only
/// sent explicitly), so a short bound would just orphan a dialog the user is still answering.
/// Callers never wait this long (their own bounds are 3-6 s); only the secrets thread does.
const PROMPT_TIMEOUT: Duration = Duration::from_secs(120);

fn unavailable(e: impl std::fmt::Display) -> SecretsError {
    SecretsError::Unavailable(e.to_string())
}

fn runtime() -> Result<tokio::runtime::Runtime, SecretsError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(SecretsError::Io)
}

pub struct SecretServiceStore {
    rt: tokio::runtime::Runtime,
    data_dir_hash: String,
    policy: Arc<UnlockPolicy>,
}

impl SecretServiceStore {
    /// Connects and checks the default collection exists; `Unavailable` if not reachable.
    pub fn open(data_dir_hash: String, policy: Arc<UnlockPolicy>) -> Result<Self, SecretsError> {
        let store = Self {
            rt: runtime()?,
            data_dir_hash,
            policy,
        };
        store.block(async |_c| Ok(()))?;
        Ok(store)
    }

    fn block<T>(
        &self,
        op: impl AsyncFnOnce(&Collection) -> Result<T, oo7::dbus::Error>,
    ) -> Result<T, SecretsError> {
        self.rt.block_on(async {
            let connect = async {
                let service = Service::new().await.map_err(unavailable)?;
                let collection = service.default_collection().await.map_err(unavailable)?;
                let locked = collection.is_locked().await.map_err(unavailable)?;
                Ok::<_, SecretsError>((collection, locked))
            };
            let (collection, locked) = tokio::time::timeout(OP_TIMEOUT, connect)
                .await
                .map_err(|_| SecretsError::Timeout)??;
            if locked {
                if !self.policy.may_prompt() {
                    return Err(SecretsError::Unavailable("the keyring is locked".into()));
                }
                tokio::time::timeout(PROMPT_TIMEOUT, collection.unlock(None))
                    .await
                    .map_err(|_| SecretsError::Timeout)?
                    .map_err(unavailable)?;
            }
            tokio::time::timeout(OP_TIMEOUT, op(&collection))
                .await
                .map_err(|_| SecretsError::Timeout)?
                .map_err(unavailable)
        })
    }

    fn attrs(&self) -> [(&'static str, String); 2] {
        [
            ("app", APP_ID.to_string()),
            ("data_dir", self.data_dir_hash.clone()),
        ]
    }
}

impl SecretStore for SecretServiceStore {
    fn load(&mut self) -> Result<Option<Secrets>, SecretsError> {
        let attrs = self.attrs();
        let bytes = self.block(async |c| {
            let items = c.search_items(&attrs).await?;
            match items.first() {
                Some(item) => Ok(Some(Zeroizing::new(
                    item.secret().await?.as_bytes().to_vec(),
                ))),
                None => Ok(None),
            }
        })?;
        match bytes {
            Some(b) => serde_json::from_slice(&b)
                .map(Some)
                .map_err(|e| SecretsError::Format(e.to_string())),
            None => Ok(None),
        }
    }

    fn save(&mut self, secrets: &Secrets) -> Result<(), SecretsError> {
        let json = Zeroizing::new(
            serde_json::to_vec(secrets).map_err(|e| SecretsError::Format(e.to_string()))?,
        );
        let attrs = self.attrs();
        self.block(async |c| {
            c.create_item(
                "fsync device secrets",
                &attrs,
                oo7::Secret::blob(&*json),
                true,
                None,
            )
            .await
            .map(|_| ())
        })
    }

    fn clear(&mut self) -> Result<(), SecretsError> {
        let attrs = self.attrs();
        self.block(async |c| {
            for item in c.search_items(&attrs).await? {
                item.delete(None).await?;
            }
            Ok(())
        })
    }
}

/// Production providers: the real session bus.
#[derive(Default)]
pub struct SystemProviders {
    policy: Arc<UnlockPolicy>,
}

impl Providers for SystemProviders {
    fn secret_service_answers(&self, timeout: Duration) -> bool {
        let Ok(rt) = runtime() else { return false };
        // The Service must be dropped inside the runtime: its Drop calls tokio::spawn to close
        // the session and panics outside a runtime context.
        rt.block_on(async {
            matches!(
                tokio::time::timeout(timeout, Service::new()).await,
                Ok(Ok(_))
            )
        })
    }

    fn open_secret_service(&self, data_dir: &Path) -> Result<Box<dyn SecretStore>, SecretsError> {
        let canonical = std::fs::canonicalize(data_dir)?;
        Ok(Box::new(SecretServiceStore::open(
            crate::paths::data_dir_hash(&canonical),
            Arc::clone(&self.policy),
        )?))
    }

    fn request_unlock(&self) {
        self.policy.request();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: the probe used to drop oo7's Service outside the runtime, and its Drop
    /// panicked ("no reactor running") whenever a real Secret Service answered.
    #[test]
    fn probe_outside_runtime_does_not_panic() {
        let _ = SystemProviders::default().secret_service_answers(Duration::from_secs(2));
    }
}
