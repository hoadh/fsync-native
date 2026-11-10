//! The `fsync-secrets` thread's job queue and worker (the handle lives in `secrets_thread`).

use super::secrets_thread::SecretsStatus;
use super::{Opened, Providers, SecretStore, Secrets, SecretsError, open_store};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::{Arc, Mutex};

/// Sequence numbers of saves: handed out when a save is queued, recorded when the worker has
/// applied one. Lets callers tell "my failed save was superseded or completed late" apart.
#[derive(Default)]
pub struct SaveTracker {
    next: AtomicU64,
    applied: AtomicU64,
    observer: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl SaveTracker {
    pub(super) fn next_seq(&self) -> u64 {
        self.next.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub(super) fn applied_seq(&self) -> u64 {
        self.applied.load(Ordering::SeqCst)
    }

    pub(super) fn set_observer(&self, f: Arc<dyn Fn() + Send + Sync>) {
        *self.observer.lock().unwrap_or_else(|p| p.into_inner()) = Some(f);
    }

    /// Called by the worker after a save reached the store (also when its caller gave up).
    fn applied_now(&self, seq: u64) {
        self.applied.fetch_max(seq, Ordering::SeqCst);
        let f = self
            .observer
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        if let Some(f) = f {
            f();
        }
    }
}

pub(super) enum Job {
    Open(SyncSender<Result<SecretsStatus, SecretsError>>),
    Load(SyncSender<Result<Option<Secrets>, SecretsError>>),
    Save(Secrets, u64, SyncSender<Result<(), SecretsError>>),
    Clear(SyncSender<Result<(), SecretsError>>),
}

pub(super) struct Worker {
    pub(super) data_dir: PathBuf,
    pub(super) providers: Arc<dyn Providers>,
    pub(super) force_file: bool,
    pub(super) store: Option<Box<dyn SecretStore>>,
    pub(super) status: Arc<Mutex<SecretsStatus>>,
    pub(super) saves: Arc<SaveTracker>,
}

impl Worker {
    /// Open (or retry opening) the backend; Ok when a store is usable.
    fn ensure_store(&mut self) -> Result<&mut Box<dyn SecretStore>, SecretsError> {
        if self.store.is_none() {
            let Opened {
                store,
                backend,
                file_fallback_warning,
            } = open_store(&self.data_dir, self.providers.as_ref(), self.force_file)?;
            let mut st = self.status.lock().unwrap_or_else(|p| p.into_inner());
            st.backend = backend.or(st.backend);
            st.waiting_for_keyring = store.is_none();
            st.file_fallback_warning |= file_fallback_warning;
            self.store = store;
        }
        self.store.as_mut().ok_or(SecretsError::WaitingForKeyring)
    }

    /// Runs a store operation; a keyring that stopped answering drops the store so the next job
    /// re-probes it (and the UI shows the keyring banner meanwhile).
    fn with_store<T>(
        &mut self,
        f: impl FnOnce(&mut Box<dyn SecretStore>) -> Result<T, SecretsError>,
    ) -> Result<T, SecretsError> {
        let out = self.ensure_store().and_then(f);
        if matches!(
            out,
            Err(SecretsError::Unavailable(_) | SecretsError::Timeout)
        ) {
            self.store = None;
            self.status
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .waiting_for_keyring = true;
        }
        out
    }

    pub(super) fn run(mut self, rx: Receiver<Job>) {
        // Replies may arrive after the caller timed out: ignore send errors.
        while let Ok(job) = rx.recv() {
            match job {
                Job::Open(r) => {
                    let out = self
                        .ensure_store()
                        .map(|_| ())
                        .or_else(|e| {
                            if matches!(e, SecretsError::WaitingForKeyring) {
                                Ok(())
                            } else {
                                Err(e)
                            }
                        })
                        .map(|()| *self.status.lock().unwrap_or_else(|p| p.into_inner()));
                    let _ = r.send(out);
                }
                Job::Load(r) => {
                    let _ = r.send(self.with_store(|s| s.load()));
                }
                Job::Save(secrets, seq, r) => {
                    let out = self.with_store(|s| s.save(&secrets));
                    if out.is_ok() {
                        self.saves.applied_now(seq);
                    }
                    let _ = r.send(out);
                }
                Job::Clear(r) => {
                    let _ = r.send(self.with_store(|s| s.clear()));
                }
            }
        }
    }
}
