//! `<data_dir>/secrets.json`, 0600: weaker store used when no Secret Service exists.

use super::{SecretStore, Secrets, SecretsError};
use rustix::process::getuid;
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

pub const FILE_NAME: &str = "secrets.json";

pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join(FILE_NAME),
        }
    }
}

impl SecretStore for FileStore {
    fn load(&mut self) -> Result<Option<Secrets>, SecretsError> {
        // O_NOFOLLOW: a planted symlink is an error, not a read of someone else's file.
        let mut f = match std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(&self.path)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(SecretsError::Io(e)),
        };
        let meta = f.metadata()?;
        if meta.uid() != getuid().as_raw() {
            return Err(SecretsError::Insecure(
                "secrets.json is not owned by the current user".into(),
            ));
        }
        if meta.mode() & 0o177 != 0 {
            return Err(SecretsError::Insecure(format!(
                "secrets.json mode is {:o}, expected 600",
                meta.mode() & 0o7777
            )));
        }
        let mut buf = Zeroizing::new(Vec::new());
        f.read_to_end(&mut buf)?;
        serde_json::from_slice(&buf)
            .map(Some)
            .map_err(|e| SecretsError::Format(e.to_string()))
    }

    fn save(&mut self, secrets: &Secrets) -> Result<(), SecretsError> {
        let json = Zeroizing::new(
            serde_json::to_vec(secrets).map_err(|e| SecretsError::Format(e.to_string()))?,
        );
        let tmp = self.path.with_extension("json.tmp");
        let _ = std::fs::remove_file(&tmp);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        let written = f.write_all(&json).and_then(|()| f.sync_all());
        if let Err(e) = written.and_then(|()| std::fs::rename(&tmp, &self.path)) {
            let _ = std::fs::remove_file(&tmp);
            return Err(SecretsError::Io(e));
        }
        Ok(())
    }

    fn clear(&mut self) -> Result<(), SecretsError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(SecretsError::Io(e)),
        }
    }
}
