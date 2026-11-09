//! Where the app keeps its files. Environment is injected so tests never touch the real one.

use std::fs::DirBuilder;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Environment lookup (`std::env::var` in production, a map in tests). Empty values count as unset.
pub type EnvFn<'a> = &'a dyn Fn(&str) -> Option<String>;

/// Real process environment.
pub fn process_env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

fn non_empty(env: EnvFn<'_>, key: &str) -> Option<String> {
    env(key).filter(|v| !v.is_empty())
}

#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    #[error("cannot determine home directory (set HOME, XDG_DATA_HOME or FSYNC_DATA_DIR)")]
    NoHome,
    #[error("{0} must be an absolute path")]
    NotAbsolute(&'static str),
    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Resolved locations for one app instance (one instance per data dir).
#[derive(Debug, Clone)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub config_file: PathBuf,
    pub autostart_dir: PathBuf,
    /// Directory that holds the IPC socket (`$XDG_RUNTIME_DIR/fsync`, else `<data_dir>/run`; never /tmp).
    pub runtime_dir: PathBuf,
}

fn abs_var(env: EnvFn<'_>, key: &'static str) -> Result<Option<PathBuf>, PathsError> {
    match non_empty(env, key) {
        None => Ok(None),
        Some(v) if Path::new(&v).is_absolute() => Ok(Some(PathBuf::from(v))),
        Some(_) => Err(PathsError::NotAbsolute(key)),
    }
}

impl Paths {
    pub fn from_env() -> Result<Self, PathsError> {
        Self::resolve(&process_env)
    }

    pub fn resolve(env: EnvFn<'_>) -> Result<Self, PathsError> {
        let home = || {
            non_empty(env, "HOME")
                .map(PathBuf::from)
                .ok_or(PathsError::NoHome)
        };
        let xdg = |key: &'static str, rel: &str| -> Result<PathBuf, PathsError> {
            match abs_var(env, key)? {
                Some(p) => Ok(p),
                None => Ok(home()?.join(rel)),
            }
        };
        let data_dir = match abs_var(env, "FSYNC_DATA_DIR")? {
            Some(p) => p,
            None => xdg("XDG_DATA_HOME", ".local/share")?.join("fsync"),
        };
        let config_home = xdg("XDG_CONFIG_HOME", ".config")?;
        let runtime_dir = match abs_var(env, "XDG_RUNTIME_DIR")? {
            Some(p) => p.join("fsync"),
            None => data_dir.join("run"),
        };
        Ok(Self {
            config_file: config_home.join("fsync").join("config.toml"),
            autostart_dir: config_home.join("autostart"),
            data_dir,
            runtime_dir,
        })
    }

    /// Create the data dir (0700) if missing; returns its canonical path.
    pub fn ensure_data_dir(&self) -> Result<PathBuf, PathsError> {
        create_private_dir(&self.data_dir)?;
        std::fs::canonicalize(&self.data_dir).map_err(|source| PathsError::Io {
            path: self.data_dir.clone(),
            source,
        })
    }
}

/// `mkdir -p` with mode 0700 for every created component.
pub fn create_private_dir(path: &Path) -> Result<(), PathsError> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|source| PathsError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    // Pre-existing dir keeps its mode; tighten ours only when it is too open to be private.
    let meta = std::fs::metadata(path).map_err(|source| PathsError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if meta.permissions().mode() & 0o077 != 0 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(
            |source| PathsError::Io {
                path: path.to_path_buf(),
                source,
            },
        )?;
    }
    Ok(())
}

/// Stable identifier of a data dir: blake3 (hex) of its canonical path bytes. The caller passes
/// an already canonical path (`Paths::ensure_data_dir`).
pub fn data_dir_hash(canonical_data_dir: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    blake3::hash(canonical_data_dir.as_os_str().as_bytes())
        .to_hex()
        .to_string()
}
