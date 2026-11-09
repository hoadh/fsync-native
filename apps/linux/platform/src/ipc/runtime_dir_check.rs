//! The directory that holds the socket must be ours and private, or IPC is disabled.

use rustix::process::getuid;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeDirError {
    #[error("{0} is a symlink")]
    Symlink(PathBuf),
    #[error("{0} is not a directory")]
    NotDir(PathBuf),
    #[error("{0} is not owned by the current user")]
    WrongOwner(PathBuf),
    #[error("{path} has mode {mode:o}, expected 700")]
    WrongMode { path: PathBuf, mode: u32 },
    #[error("runtime dir {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Create `dir` (0700) if missing, then require: not a symlink, a directory, owned by us, mode
/// exactly 0700. Never chmods an existing dir: a wrong mode means someone else touched it.
pub fn ensure_runtime_dir(dir: &Path) -> Result<(), RuntimeDirError> {
    let io = |source| RuntimeDirError::Io {
        path: dir.to_path_buf(),
        source,
    };
    if dir.symlink_metadata().is_err() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(io)?;
    }
    let meta = dir.symlink_metadata().map_err(io)?;
    if meta.file_type().is_symlink() {
        return Err(RuntimeDirError::Symlink(dir.to_path_buf()));
    }
    if !meta.is_dir() {
        return Err(RuntimeDirError::NotDir(dir.to_path_buf()));
    }
    if meta.uid() != getuid().as_raw() {
        return Err(RuntimeDirError::WrongOwner(dir.to_path_buf()));
    }
    let mode = meta.mode() & 0o7777;
    if mode != 0o700 {
        return Err(RuntimeDirError::WrongMode {
            path: dir.to_path_buf(),
            mode,
        });
    }
    Ok(())
}
