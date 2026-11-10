//! `<data_dir>/secrets-backend`: which backend owns this data dir's keys. Decided once so a slow
//! or locked keyring at a later launch can never lead to fresh keys (new node id, HistoryKeyMismatch).

use super::SecretsError;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

const MARKER: &str = "secrets-backend";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    SecretService,
    File,
}

impl Backend {
    fn as_str(self) -> &'static str {
        match self {
            Backend::SecretService => "secret-service",
            Backend::File => "file",
        }
    }
}

pub fn read_marker(data_dir: &Path) -> Result<Option<Backend>, SecretsError> {
    match std::fs::read_to_string(data_dir.join(MARKER)) {
        Ok(s) => match s.trim() {
            "secret-service" => Ok(Some(Backend::SecretService)),
            "file" => Ok(Some(Backend::File)),
            other => Err(SecretsError::Format(format!(
                "unknown secrets-backend marker {other:?}"
            ))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(SecretsError::Io(e)),
    }
}

/// Atomic tmp + rename, 0600.
pub fn write_marker(data_dir: &Path, backend: Backend) -> Result<(), SecretsError> {
    let tmp = data_dir.join(format!("{MARKER}.tmp"));
    let _ = std::fs::remove_file(&tmp);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(backend.as_str().as_bytes())?;
    f.sync_all()?;
    std::fs::rename(&tmp, data_dir.join(MARKER))?;
    Ok(())
}

/// Files and directories core leaves for a history (database, key check, moved-aside copies,
/// blobs), relative to `<data_dir>` and `<data_dir>/core`.
fn is_history_entry(name: &str) -> bool {
    name == "history.db"
        || name == "history.keycheck"
        || name == "blobs"
        || name.starts_with("history.corrupt-")
}

fn dir_has_history(dir: &Path) -> bool {
    match dir.try_exists() {
        Ok(false) => return false,
        Ok(true) => {}
        Err(_) => return true, // cannot tell: never risk replacing keys
    }
    match std::fs::read_dir(dir) {
        // An unreadable directory may hide a history: fail closed.
        Err(_) => true,
        Ok(rd) => rd
            .flatten()
            .any(|e| is_history_entry(&e.file_name().to_string_lossy())),
    }
}

/// New device/history keys may be generated only while the data dir has no engine history: the
/// keys that encrypted it must never be replaced. The marker is deliberately not consulted: it
/// is written when the backend opens, before the first key save, so a failed first save must
/// not leave the data dir unable to create keys (the caller also requires an empty store).
/// Unknown (an I/O error while looking) counts as "history present".
pub fn may_create_keys(data_dir: &Path) -> bool {
    !dir_has_history(data_dir) && !dir_has_history(&data_dir.join("core"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_roundtrip_and_key_creation_gate() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(read_marker(d.path()).unwrap(), None);
        assert!(may_create_keys(d.path()));
        write_marker(d.path(), Backend::File).unwrap();
        assert_eq!(read_marker(d.path()).unwrap(), Some(Backend::File));
        assert!(may_create_keys(d.path()), "a marker alone is not history");
    }

    #[test]
    fn history_db_blocks_key_creation() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("core")).unwrap();
        std::fs::write(d.path().join("core/history.db"), b"x").unwrap();
        assert!(!may_create_keys(d.path()));
    }

    #[test]
    fn every_history_leftover_blocks_key_creation() {
        for name in [
            "history.keycheck",
            "history.corrupt-123",
            "blobs",
            "history.db",
        ] {
            let d = tempfile::tempdir().unwrap();
            std::fs::create_dir(d.path().join("core")).unwrap();
            std::fs::write(d.path().join("core").join(name), b"x").unwrap();
            assert!(!may_create_keys(d.path()), "{name}");
        }
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("secrets-backend"), b"x").unwrap();
        assert!(may_create_keys(d.path()));
    }

    #[test]
    fn unreadable_data_dir_fails_closed() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let core = d.path().join("core");
        std::fs::create_dir(&core).unwrap();
        std::fs::set_permissions(&core, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Root ignores modes; only assert where the restriction is real.
        let restricted = std::fs::read_dir(&core).is_err();
        let allowed = may_create_keys(d.path());
        std::fs::set_permissions(&core, std::fs::Permissions::from_mode(0o700)).unwrap();
        if restricted {
            assert!(!allowed);
        }
    }

    #[test]
    fn garbage_marker_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(MARKER), "banana").unwrap();
        assert!(read_marker(d.path()).is_err());
    }
}
