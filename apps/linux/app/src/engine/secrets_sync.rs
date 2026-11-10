//! Keeping the secrets store in step with the in-memory mirror. All keyring access goes through
//! the `fsync-secrets` thread with a bounded wait; a timeout keeps the data in memory and raises
//! the "not saved" banner (retried on the next change and at quit).

use crate::core_state::{Core, lock};
use fsync_linux_platform::secrets::{Secrets, SessionTokens};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// The core callback thread must not wait longer than this for a token save.
pub const CALLBACK_SAVE_WAIT: Duration = Duration::from_secs(3);
pub const OP_SAVE_WAIT: Duration = Duration::from_secs(5);

/// Applies `f` to the mirror and saves the result. Returns whether the save succeeded.
///
/// The lock only covers "update mirror + queue the save": the secrets thread is FIFO, so queue
/// order is update order, and nobody holds the lock while waiting on the keyring.
pub fn persist(core: &Core, wait: Duration, f: impl FnOnce(&mut Secrets)) -> bool {
    let Some(handle) = core.secrets.as_ref() else {
        return false;
    };
    let (seq, queued) = {
        let _order = lock(&core.persist_lock);
        let snapshot = {
            let mut mem = lock(&core.secrets_mem);
            let Some(secrets) = mem.as_mut() else {
                return false;
            };
            f(secrets);
            secrets.clone()
        };
        handle.save_queued(snapshot)
    };
    let saved = match queued.and_then(|reply| reply.wait(wait)) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("secrets not saved: {e}");
            core.failed_save_seq.fetch_max(seq, Ordering::SeqCst);
            false
        }
    };
    refresh_save_state(core);
    saved
}

/// Re-derives the "not saved" banner and the keyring state from the secrets thread. The banner
/// is up while the newest failed save is not yet covered by an applied one, so a later save
/// (each snapshot holds the full state) or a late success of the failed one clears it. Also
/// runs from the secrets thread after every applied save (`watch_saves`).
pub fn refresh_save_state(core: &Core) {
    let Some(handle) = core.secrets.as_ref() else {
        return;
    };
    let unsaved = handle.applied_save_seq() < core.failed_save_seq.load(Ordering::SeqCst);
    let status = handle.status();
    core.update_ui(|u| {
        u.warnings.secrets_unsaved = unsaved;
        u.warnings.waiting_keyring = status.waiting_for_keyring;
        u.warnings.keyring_backend =
            status.backend == Some(fsync_linux_platform::secrets::Backend::SecretService);
    });
}

/// Makes late successes (a save whose caller timed out) clear the banner without another change.
pub fn watch_saves(core: &Arc<Core>) {
    let Some(handle) = core.secrets.as_ref() else {
        return;
    };
    // Weak: the core owns the handle that owns this closure.
    let weak = Arc::downgrade(core);
    handle.set_save_observer(Arc::new(move || {
        if let Some(core) = weak.upgrade() {
            refresh_save_state(&core);
        }
    }));
}

/// Current mirror value of a field, without exposing the lock.
pub fn read<T>(core: &Core, f: impl FnOnce(&Secrets) -> T) -> Option<T> {
    lock(&core.secrets_mem).as_ref().map(f)
}

pub fn to_core_tokens(t: &SessionTokens) -> fsync_core::service::SessionTokens {
    fsync_core::service::SessionTokens {
        access: t.access.clone(),
        refresh: t.refresh.clone(),
        account_id: t.account_id.clone(),
    }
}

pub fn from_core_tokens(t: &fsync_core::service::SessionTokens) -> SessionTokens {
    SessionTokens {
        access: t.access.clone(),
        refresh: t.refresh.clone(),
        account_id: t.account_id.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fsync_linux_platform::config_file::AppConfig;
    use fsync_linux_platform::paths::Paths;

    fn core_without_store() -> Core {
        let dir = std::path::PathBuf::from("/nonexistent/fsync-secrets-sync-test");
        let paths = Paths {
            data_dir: dir.clone(),
            config_file: dir.join("config.toml"),
            autostart_dir: dir.join("autostart"),
            runtime_dir: dir.join("run"),
        };
        let cfg = AppConfig {
            api: "http://127.0.0.1:9".into(),
            relays: Vec::new(),
            device_name: "t".into(),
            allow_insecure_http: true,
        };
        Core::new(paths, dir, cfg, None)
    }

    #[test]
    fn token_mapping_keeps_each_field_in_place() {
        // Distinct values per field: a swapped access/refresh mapping must not pass.
        let core_side = fsync_core::service::SessionTokens {
            access: "acc".into(),
            refresh: "ref".into(),
            account_id: "acct".into(),
        };
        let app_side = from_core_tokens(&core_side);
        assert_eq!(
            (
                app_side.access.as_str(),
                app_side.refresh.as_str(),
                app_side.account_id.as_str()
            ),
            ("acc", "ref", "acct")
        );
        let back = to_core_tokens(&app_side);
        assert_eq!(
            (
                back.access.as_str(),
                back.refresh.as_str(),
                back.account_id.as_str()
            ),
            ("acc", "ref", "acct")
        );
    }

    #[test]
    fn read_and_persist_are_inert_without_a_store_or_mirror() {
        let core = core_without_store();
        // No secrets thread (smoke build) and no mirror: nothing is read, nothing is saved,
        // and the closure is never run.
        assert_eq!(read(&core, |s| s.email.clone()), None);
        let mut ran = false;
        let saved = persist(&core, OP_SAVE_WAIT, |_| ran = true);
        assert!(!saved);
        assert!(!ran, "mutation must not run without a store");
        assert!(!lock(&core.ui).warnings.secrets_unsaved);
    }
}
