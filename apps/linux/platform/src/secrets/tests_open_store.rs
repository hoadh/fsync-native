//! Backend choice with a fake bus: never talks to the real session bus.

use super::*;
use std::sync::Mutex;

struct MemStore(Option<Secrets>);
impl SecretStore for MemStore {
    fn load(&mut self) -> Result<Option<Secrets>, SecretsError> {
        Ok(self.0.clone())
    }
    fn save(&mut self, s: &Secrets) -> Result<(), SecretsError> {
        self.0 = Some(s.clone());
        Ok(())
    }
    fn clear(&mut self) -> Result<(), SecretsError> {
        self.0 = None;
        Ok(())
    }
}

pub struct FakeBus {
    pub answers: bool,
    pub usable: bool,
    pub probes: Mutex<u32>,
}

impl FakeBus {
    pub fn new(answers: bool, usable: bool) -> Self {
        Self {
            answers,
            usable,
            probes: Mutex::new(0),
        }
    }
}

impl Providers for FakeBus {
    fn secret_service_answers(&self, _t: Duration) -> bool {
        *self.probes.lock().unwrap() += 1;
        self.answers
    }
    fn open_secret_service(&self, _d: &Path) -> Result<Box<dyn SecretStore>, SecretsError> {
        if self.usable {
            Ok(Box::new(MemStore(None)))
        } else {
            Err(SecretsError::Unavailable("fake: locked".into()))
        }
    }
}

#[test]
fn first_run_without_secret_service_uses_file_and_writes_marker() {
    let d = tempfile::tempdir().unwrap();
    let o = open_store(d.path(), &FakeBus::new(false, false), false).unwrap();
    assert_eq!(o.backend, Some(Backend::File));
    assert!(o.file_fallback_warning && o.store.is_some());
    assert_eq!(
        backend_marker::read_marker(d.path()).unwrap(),
        Some(Backend::File)
    );
}

#[test]
fn first_run_with_secret_service_writes_marker() {
    let d = tempfile::tempdir().unwrap();
    let o = open_store(d.path(), &FakeBus::new(true, true), false).unwrap();
    assert_eq!(o.backend, Some(Backend::SecretService));
    assert!(!o.file_fallback_warning);
    assert_eq!(
        backend_marker::read_marker(d.path()).unwrap(),
        Some(Backend::SecretService)
    );
}

#[test]
fn marker_present_and_keyring_unavailable_waits_without_creating_keys() {
    let d = tempfile::tempdir().unwrap();
    backend_marker::write_marker(d.path(), Backend::SecretService).unwrap();
    // Even though the bus "does not answer", the marker wins: no fallback, no probe.
    let bus = FakeBus::new(false, false);
    let o = open_store(d.path(), &bus, false).unwrap();
    assert!(o.store.is_none());
    assert_eq!(o.backend, Some(Backend::SecretService));
    assert_eq!(*bus.probes.lock().unwrap(), 0);
    assert!(!d.path().join(file_fallback::FILE_NAME).exists());
    assert_eq!(
        backend_marker::read_marker(d.path()).unwrap(),
        Some(Backend::SecretService)
    );
}

#[test]
fn first_run_answering_but_locked_waits_and_leaves_no_marker() {
    let d = tempfile::tempdir().unwrap();
    let o = open_store(d.path(), &FakeBus::new(true, false), false).unwrap();
    assert!(o.store.is_none() && o.backend.is_none());
    assert_eq!(backend_marker::read_marker(d.path()).unwrap(), None);
}

#[test]
fn file_marker_never_probes_bus() {
    let d = tempfile::tempdir().unwrap();
    backend_marker::write_marker(d.path(), Backend::File).unwrap();
    let bus = FakeBus::new(true, true);
    let o = open_store(d.path(), &bus, false).unwrap();
    assert_eq!(o.backend, Some(Backend::File));
    assert_eq!(*bus.probes.lock().unwrap(), 0);
}

#[test]
fn force_file_skips_bus_and_records_marker() {
    let d = tempfile::tempdir().unwrap();
    let bus = FakeBus::new(true, true);
    let o = open_store(d.path(), &bus, true).unwrap();
    assert_eq!(o.backend, Some(Backend::File));
    assert_eq!(*bus.probes.lock().unwrap(), 0);
    assert_eq!(
        backend_marker::read_marker(d.path()).unwrap(),
        Some(Backend::File)
    );
}
