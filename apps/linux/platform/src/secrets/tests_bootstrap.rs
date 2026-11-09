//! Fake providers only: the real keyring is never touched.

use super::super::{Providers, SecretStore};
use super::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// Keyring whose contents survive reopening, with a switch that fails saves.
#[derive(Default)]
struct Shared {
    item: Mutex<Option<Secrets>>,
    fail_saves: AtomicU32,
}

struct SharedStore(Arc<Shared>);

impl SecretStore for SharedStore {
    fn load(&mut self) -> Result<Option<Secrets>, SecretsError> {
        Ok(self.0.item.lock().unwrap().clone())
    }
    fn save(&mut self, s: &Secrets) -> Result<(), SecretsError> {
        if self.0.fail_saves.load(Ordering::SeqCst) > 0 {
            self.0.fail_saves.fetch_sub(1, Ordering::SeqCst);
            return Err(SecretsError::Unavailable("fake: prompt dismissed".into()));
        }
        *self.0.item.lock().unwrap() = Some(s.clone());
        Ok(())
    }
    fn clear(&mut self) -> Result<(), SecretsError> {
        *self.0.item.lock().unwrap() = None;
        Ok(())
    }
}

struct SharedBus(Arc<Shared>);

impl Providers for SharedBus {
    fn secret_service_answers(&self, _t: Duration) -> bool {
        true
    }
    fn open_secret_service(&self, _d: &Path) -> Result<Box<dyn SecretStore>, SecretsError> {
        Ok(Box::new(SharedStore(Arc::clone(&self.0))))
    }
}

fn handle(dir: &Path, shared: &Arc<Shared>) -> SecretsHandle {
    SecretsHandle::spawn(dir.into(), Arc::new(SharedBus(Arc::clone(shared))), false).unwrap()
}

fn fixed() -> Result<[u8; 32], SecretsError> {
    Ok([9; 32])
}

const WAIT: Duration = Duration::from_secs(3);

#[test]
fn failed_first_save_does_not_brick_the_data_dir() {
    let d = tempfile::tempdir().unwrap();
    let shared = Arc::new(Shared::default());
    shared.fail_saves.store(1, Ordering::SeqCst);
    let h = handle(d.path(), &shared);
    h.ensure_open().unwrap();
    // The backend marker exists now (written when the store opened) but no key was saved.
    assert!(d.path().join("secrets-backend").exists());
    let first = load_or_create(&h, d.path(), WAIT, &fixed);
    assert!(matches!(first, Err(SecretsError::Unavailable(_))));
    // The worker dropped the failed store; the retry re-opens it and creates the keys.
    h.ensure_open().unwrap();
    let again = load_or_create(&h, d.path(), WAIT, &fixed).unwrap();
    assert_eq!(again.device_secret, [9; 32]);
    assert!(shared.item.lock().unwrap().is_some());
    // And from now on the stored keys are returned, not regenerated.
    let third = load_or_create(&h, d.path(), WAIT, &|| Ok([1; 32])).unwrap();
    assert_eq!(third.history_key, [9; 32]);
}

#[test]
fn history_present_and_empty_store_refuses_to_create_keys() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir(d.path().join("core")).unwrap();
    std::fs::write(d.path().join("core/history.db"), b"x").unwrap();
    let shared = Arc::new(Shared::default());
    let h = handle(d.path(), &shared);
    h.ensure_open().unwrap();
    let r = load_or_create(&h, d.path(), WAIT, &fixed);
    assert!(matches!(r, Err(SecretsError::KeysMissing)));
    assert!(
        shared.item.lock().unwrap().is_none(),
        "nothing may be saved"
    );
}
