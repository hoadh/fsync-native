//! Atomic private-file writes (0600 on unix).

use crate::error::{CoreError, Result};
use std::{fs, io::Write, path::Path};

/// 16 hex chars from the OS RNG, for unguessable temp names.
pub fn random_suffix() -> Result<String> {
    let mut b = [0u8; 8];
    getrandom::fill(&mut b).map_err(|e| CoreError::Store(format!("rng: {e}")))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

/// Write `bytes` to `path` via a temp file + fsync + rename; file mode 0600. The temp file is
/// `<name>.<random>.tmp`, created exclusively (never follows a planted symlink) and removed on
/// failure.
pub fn write_private_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let tmp = dir.join(format!("{name}.{}.tmp", random_suffix()?));
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let written = (|| -> Result<()> {
        let mut f = opts.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        Ok(fs::rename(&tmp, path)?)
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// Positional read (no shared cursor), portable across unix and windows.
pub fn pread(f: &fs::File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    #[cfg(unix)]
    {
        std::os::unix::fs::FileExt::read_at(f, buf, offset)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::FileExt::seek_read(f, buf, offset)
    }
}

/// Create `dir` (and parents) and restrict it to the owner (0700 on unix).
pub fn ensure_private_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Bytes a non-privileged writer can still add on the volume holding `path`.
pub fn available_space(path: &Path) -> Result<u64> {
    Ok(fs4::available_space(path)?)
}
