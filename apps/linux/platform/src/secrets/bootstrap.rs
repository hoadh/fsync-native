//! First-run key creation. Keys are generated only while the data dir has no engine history AND
//! the store is empty; a failed save leaves nothing behind that blocks the next attempt.

use super::{Secrets, SecretsError, SecretsHandle, backend_marker::may_create_keys};
use std::path::Path;
use std::time::Duration;

/// Loads the stored secrets, or creates fresh keys on a pristine data dir.
///
/// - `Err(KeysMissing)`: the store is empty but history exists (never replace those keys).
/// - Any save/load error is returned as is; the caller decides between waiting and failing, and a
///   later call simply tries again (nothing is recorded before the first successful save).
pub fn load_or_create(
    handle: &SecretsHandle,
    data_dir: &Path,
    save_wait: Duration,
    random32: &dyn Fn() -> Result<[u8; 32], SecretsError>,
) -> Result<Secrets, SecretsError> {
    if let Some(found) = handle.load()? {
        return Ok(found);
    }
    if !may_create_keys(data_dir) {
        return Err(SecretsError::KeysMissing);
    }
    let secrets = Secrets {
        device_secret: random32()?,
        history_key: random32()?,
        session: None,
        email: None,
        pending_revoke: None,
    };
    handle.save(secrets.clone(), save_wait)?;
    Ok(secrets)
}

#[cfg(test)]
#[path = "tests_bootstrap.rs"]
mod tests;
