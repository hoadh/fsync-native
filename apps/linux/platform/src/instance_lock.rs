//! One app per data dir: non-blocking exclusive flock on `<data_dir>/app.lock`, held for the
//! process lifetime. Only the holder may unlink/bind the IPC socket.

use rustix::fs::{FlockOperation, flock};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("another fsync instance holds the lock for this data dir")]
    Held,
    #[error("cannot open lock file: {0}")]
    Io(#[from] std::io::Error),
}

/// Releases the lock when dropped (or when the process exits).
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    pub fn acquire(data_dir: &Path) -> Result<Self, LockError> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(data_dir.join("app.lock"))?;
        match flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => Ok(Self { _file: file }),
            Err(e) if e == rustix::io::Errno::WOULDBLOCK => Err(LockError::Held),
            Err(e) => Err(LockError::Io(e.into())),
        }
    }

    /// Probe without keeping the lock: true when another holder exists (used by the IPC client).
    pub fn is_held(data_dir: &Path) -> bool {
        matches!(Self::acquire(data_dir), Err(LockError::Held))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_is_held_until_drop() {
        let d = tempfile::tempdir().unwrap();
        let a = InstanceLock::acquire(d.path()).unwrap();
        assert!(matches!(
            InstanceLock::acquire(d.path()),
            Err(LockError::Held)
        ));
        assert!(InstanceLock::is_held(d.path()));
        drop(a);
        assert!(InstanceLock::acquire(d.path()).is_ok());
    }
}
