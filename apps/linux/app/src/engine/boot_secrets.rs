//! Secrets step of the boot: marker-driven load (never creating keys on a data dir that had
//! any), waiting for a slow or locked keyring instead of failing.

use super::boot::fail;
use super::secrets_sync::OP_SAVE_WAIT;
use crate::core_state::Core;
use crate::core_state::ui_state::EngineState;
use fsync_linux_platform::secrets::{
    Backend, Secrets, SecretsError, SecretsHandle, load_or_create,
};
use std::time::Duration;

const KEYRING_RETRY: Duration = Duration::from_secs(10);

fn random32() -> Result<[u8; 32], SecretsError> {
    let mut b = [0u8; 32];
    let mut filled = 0;
    while filled < b.len() {
        filled += rustix::rand::getrandom(&mut b[filled..], rustix::rand::GetRandomFlags::empty())
            .map_err(|e| SecretsError::Io(std::io::Error::from(e)))?;
    }
    Ok(b)
}

/// The keyring being slow, locked or gone is a reason to wait and retry, never to fail.
fn keyring_not_ready(e: &SecretsError) -> bool {
    matches!(
        e,
        SecretsError::WaitingForKeyring | SecretsError::Unavailable(_) | SecretsError::Timeout
    )
}

enum Step {
    Got(Secrets),
    Wait,
    Fatal(String),
}

fn try_obtain(core: &Core, handle: &SecretsHandle) -> Step {
    let status = match handle.ensure_open() {
        Ok(s) => s,
        Err(e) if keyring_not_ready(&e) => return Step::Wait,
        Err(e) => return Step::Fatal(format!("Cannot open the secrets store: {e}")),
    };
    core.update_ui(|u| {
        u.warnings.file_fallback = status.file_fallback_warning;
        u.warnings.waiting_keyring = status.waiting_for_keyring;
        u.warnings.keyring_backend = status.backend == Some(Backend::SecretService);
    });
    if status.waiting_for_keyring {
        return Step::Wait;
    }
    match load_or_create(handle, &core.data_dir, OP_SAVE_WAIT, &random32) {
        Ok(s) => Step::Got(s),
        Err(SecretsError::KeysMissing) => Step::Fatal(format!(
            "The stored keys for this data directory are missing, so the existing history \
                 cannot be opened. Nothing was changed. If you want to start fresh, quit fsync \
                 and move the folder {} aside, then start it again.",
            core.data_dir.display()
        )),
        Err(e) if keyring_not_ready(&e) => Step::Wait,
        Err(e) => Step::Fatal(format!("Cannot read the secrets store: {e}")),
    }
}

/// Loads (or on a pristine data dir creates) the secrets, waiting for the keyring as long as it
/// takes. `None`: shutdown began or a fatal problem was already shown.
pub(super) fn obtain_secrets(core: &Core) -> Option<Secrets> {
    let handle = core.secrets.as_ref()?;
    loop {
        if !core.gate.is_open() {
            return None;
        }
        match try_obtain(core, handle) {
            Step::Got(s) => return Some(s),
            Step::Fatal(m) => {
                fail(core, m);
                return None;
            }
            Step::Wait => {}
        }
        core.update_ui(|u| {
            u.state = EngineState::WaitingForKeyring;
            u.warnings.waiting_keyring = true;
        });
        // Poll every 10 s; "Unlock keyring" and shutdown cut the wait short.
        core.boot.sleep(KEYRING_RETRY);
    }
}
