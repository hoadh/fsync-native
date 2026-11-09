//! Files that are never published from a clipboard `text/uri-list` (or a "send to"): the
//! confused-deputy guard. Any client can set a uri-list, so keys, keyrings, password stores and
//! our own data must not leave the machine through it. Only paths under `$HOME`, /media, /mnt
//! and /run/media are ever allowed, and credential / browser-profile folders under `$HOME`
//! (~/.aws, ~/.kube, ~/.docker, ~/.netrc, ~/.config/gh, ~/.mozilla, chromium, chrome, brave) are
//! protected like ~/.ssh. Flatpak app data (~/.var/app) is not listed: documented limit.
//! Checked after canonicalize, so a symlink into `~/.ssh` is caught. A folder that contains a
//! denied root (for example `~`) is refused too, because core expands directories.

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PathDenied {
    #[error("the path does not exist or cannot be resolved")]
    Unresolvable,
    #[error("the path is protected (keys, keyrings or fsync data)")]
    Protected,
}

#[derive(Clone, Debug)]
pub struct PathDenylist {
    roots: Vec<PathBuf>,
    /// Only paths under one of these may be published (empty = fail closed, HOME unknown).
    allowed: Vec<PathBuf>,
}

/// Mount points of removable media, allowed next to `$HOME`.
const ALLOWED_OUTSIDE_HOME: &[&str] = &["/media", "/mnt", "/run/media"];

/// Credentials and browser profiles under `$HOME`.
const HOME_PROTECTED: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".local/share/keyrings",
    ".password-store",
    ".config/fsync",
    ".aws",
    ".kube",
    ".docker",
    ".netrc",
    ".config/gh",
    ".local/share/kwalletd",
    ".mozilla",
    ".config/chromium",
    ".config/google-chrome",
    ".config/BraveSoftware",
];

fn canon_or_lexical(p: PathBuf) -> PathBuf {
    // A root that does not exist cannot be reached through a symlink either; keep the lexical
    // form so a later-created directory is still covered.
    p.canonicalize().unwrap_or(p)
}

impl PathDenylist {
    /// `data_dir`: the app's data directory; `home` and the XDG dirs come from the caller so
    /// tests never read the environment.
    pub fn new(home: &Path, data_dir: &Path, xdg_data: &Path, xdg_config: &Path) -> Self {
        let mut raw = vec![
            data_dir.to_path_buf(),
            xdg_data.join("keyrings"),
            xdg_config.join("fsync"),
            PathBuf::from("/proc"),
            PathBuf::from("/sys"),
            PathBuf::from("/dev"),
        ];
        raw.extend(HOME_PROTECTED.iter().map(|r| home.join(r)));
        let mut roots: Vec<PathBuf> = Vec::new();
        for r in raw {
            let c = canon_or_lexical(r);
            if !roots.contains(&c) {
                roots.push(c);
            }
        }
        let allowed = std::iter::once(home.to_path_buf())
            .chain(ALLOWED_OUTSIDE_HOME.iter().map(PathBuf::from))
            .map(canon_or_lexical)
            .collect();
        Self { roots, allowed }
    }

    /// Refuses every path (HOME is unknown, so the protected places cannot be named).
    pub fn closed() -> Self {
        Self {
            roots: Vec::new(),
            allowed: Vec::new(),
        }
    }

    /// From the process environment (HOME, XDG_DATA_HOME, XDG_CONFIG_HOME with their
    /// defaults). Without HOME it fails closed.
    pub fn from_env(env: &dyn Fn(&str) -> Option<String>, data_dir: &Path) -> Self {
        let var = |k: &str| env(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let Some(home) = var("HOME").filter(|h| h.is_absolute()) else {
            return Self::closed();
        };
        let xdg_data = var("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
        let xdg_config = var("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        Self::new(&home, data_dir, &xdg_data, &xdg_config)
    }

    /// Canonical path when allowed: under `$HOME` or a media / scratch mount, not under (and
    /// not containing) a protected root.
    pub fn check(&self, path: &Path) -> Result<PathBuf, PathDenied> {
        if !path.is_absolute() {
            return Err(PathDenied::Unresolvable);
        }
        let canon = path.canonicalize().map_err(|_| PathDenied::Unresolvable)?;
        if !self.allowed.iter().any(|a| canon.starts_with(a)) {
            return Err(PathDenied::Protected);
        }
        if self
            .roots
            .iter()
            .any(|r| canon.starts_with(r) || r.starts_with(&canon))
        {
            return Err(PathDenied::Protected);
        }
        Ok(canon)
    }
}

#[cfg(test)]
#[path = "path_denylist_tests.rs"]
mod tests;
