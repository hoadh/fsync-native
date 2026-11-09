//! Dedicated `fsync-secrets` thread: all keyring I/O (blocking D-Bus) happens here, never on the
//! engine callback thread. Callers get a `Result` within a bound and never hang on the keyring.

use super::reply::Reply;
use super::worker::{Job, SaveTracker, Worker};
use super::{Backend, Providers, Secrets, SecretsError};
use std::path::PathBuf;
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Per-job bound for `load`/`clear`/`ensure_open`.
pub const JOB_TIMEOUT: Duration = Duration::from_secs(5);

/// Snapshot for the UI (banners, state).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SecretsStatus {
    /// Backend chosen for this data dir; None until decided.
    pub backend: Option<Backend>,
    /// True while the backend (Secret Service) cannot be used: caller shows the keyring banner.
    pub waiting_for_keyring: bool,
    /// First run fell back to the 0600 file: caller shows the persistent warning.
    pub file_fallback_warning: bool,
}

#[derive(Clone)]
pub struct SecretsHandle {
    tx: mpsc::Sender<Job>,
    providers: Arc<dyn Providers>,
    status: Arc<Mutex<SecretsStatus>>,
    saves: Arc<SaveTracker>,
}

impl SecretsHandle {
    /// Start the thread. Nothing touches the keyring until the first job (`ensure_open`).
    pub fn spawn(
        data_dir: PathBuf,
        providers: Arc<dyn Providers>,
        force_file: bool,
    ) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(SecretsStatus::default()));
        let saves = Arc::new(SaveTracker::default());
        let worker = Worker {
            data_dir,
            providers: Arc::clone(&providers),
            force_file,
            store: None,
            status: Arc::clone(&status),
            saves: Arc::clone(&saves),
        };
        std::thread::Builder::new()
            .name("fsync-secrets".into())
            .spawn(move || worker.run(rx))?;
        Ok(Self {
            tx,
            providers,
            status,
            saves,
        })
    }

    pub fn status(&self) -> SecretsStatus {
        *self.status.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Queues a job (FIFO: jobs run in the order they were queued).
    fn enqueue<T>(
        &self,
        make: impl FnOnce(SyncSender<Result<T, SecretsError>>) -> Job,
    ) -> Result<Reply<T>, SecretsError> {
        let (rtx, rrx) = mpsc::sync_channel(1);
        self.tx
            .send(make(rtx))
            .map_err(|_| SecretsError::ThreadGone)?;
        Ok(Reply(rrx))
    }

    fn call<T>(
        &self,
        make: impl FnOnce(SyncSender<Result<T, SecretsError>>) -> Job,
        wait: Duration,
    ) -> Result<T, SecretsError> {
        self.enqueue(make)?.wait(wait)
    }

    /// The user pressed "Unlock keyring": the next keyring access may show one unlock prompt.
    pub fn request_unlock(&self) {
        self.providers.request_unlock();
    }

    /// Decide/open the backend (first run probes the bus, up to ~2 s). Also the retry entry point
    /// while `waiting_for_keyring` (every 10 s and on NameOwnerChanged).
    pub fn ensure_open(&self) -> Result<SecretsStatus, SecretsError> {
        self.call(
            Job::Open,
            JOB_TIMEOUT.saturating_add(Duration::from_secs(1)),
        )
    }

    /// `ensure_open` with the caller's own bound (the user-initiated unlock waits for a prompt).
    pub fn ensure_open_within(&self, wait: Duration) -> Result<SecretsStatus, SecretsError> {
        self.call(Job::Open, wait)
    }

    /// `Err(WaitingForKeyring)` while the keyring is unavailable.
    pub fn load(&self) -> Result<Option<Secrets>, SecretsError> {
        self.call(Job::Load, JOB_TIMEOUT)
    }

    /// `wait` is the caller's own bound (callback thread: 3 s). On `Timeout` the save may still
    /// complete later; the caller keeps tokens in memory and retries.
    pub fn save(&self, secrets: Secrets, wait: Duration) -> Result<(), SecretsError> {
        let seq = self.saves.next_seq();
        self.call(|r| Job::Save(secrets, seq, r), wait)
    }

    /// Queues a save without waiting; returns its sequence number and the pending answer.
    /// Callers that need saves applied in update order queue them under their own lock (cheap)
    /// and wait on the `Reply` outside of it.
    pub fn save_queued(&self, secrets: Secrets) -> (u64, Result<Reply<()>, SecretsError>) {
        let seq = self.saves.next_seq();
        (seq, self.enqueue(|r| Job::Save(secrets, seq, r)))
    }

    /// Sequence number of the newest save the store has accepted (0: none yet).
    pub fn applied_save_seq(&self) -> u64 {
        self.saves.applied_seq()
    }

    /// `f` runs on the secrets thread after every save the store accepted, including ones whose
    /// caller already gave up waiting. It must not block.
    pub fn set_save_observer(&self, f: Arc<dyn Fn() + Send + Sync>) {
        self.saves.set_observer(f);
    }

    pub fn clear(&self) -> Result<(), SecretsError> {
        self.call(Job::Clear, JOB_TIMEOUT)
    }
}

#[cfg(test)]
#[path = "tests_thread.rs"]
mod tests;
