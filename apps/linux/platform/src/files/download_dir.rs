//! Where pushed files are saved: `<XDG download dir>/fsync`.

use crate::paths::EnvFn;
use std::path::{Path, PathBuf};

/// `XDG_DOWNLOAD_DIR` from the environment or `user-dirs.dirs`, else `~/Downloads`; always
/// absolute. `None` only without a usable HOME.
pub fn download_root(env: EnvFn<'_>) -> Option<PathBuf> {
    let home = env("HOME").filter(|h| Path::new(h).is_absolute())?;
    let abs = |v: String| Path::new(&v).is_absolute().then(|| PathBuf::from(v));
    if let Some(p) = env("XDG_DOWNLOAD_DIR")
        .filter(|v| !v.is_empty())
        .and_then(abs)
    {
        return Some(p);
    }
    let config = env("XDG_CONFIG_HOME")
        .filter(|v| Path::new(v).is_absolute())
        .map_or_else(|| Path::new(&home).join(".config"), PathBuf::from);
    let from_file = std::fs::read_to_string(config.join("user-dirs.dirs"))
        .ok()
        .and_then(|text| parse_user_dirs(&text, &home));
    Some(from_file.unwrap_or_else(|| Path::new(&home).join("Downloads")))
}

/// The directory pushed files are saved into.
pub fn push_dir(env: EnvFn<'_>) -> Option<PathBuf> {
    download_root(env).map(|d| d.join("fsync"))
}

/// `XDG_DOWNLOAD_DIR="$HOME/Downloads"` lines; only `$HOME` is expanded.
fn parse_user_dirs(text: &str, home: &str) -> Option<PathBuf> {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("XDG_DOWNLOAD_DIR="))?;
    let value = line.split_once('=')?.1.trim().trim_matches('"');
    let expanded = match value.strip_prefix("$HOME") {
        Some(rest) => format!("{home}{rest}"),
        None => value.to_owned(),
    };
    Path::new(&expanded)
        .is_absolute()
        .then(|| PathBuf::from(expanded))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn falls_back_to_downloads_and_honours_env_and_user_dirs() {
        let dir = tempfile::tempdir().expect("tmp");
        let cfg = dir.path().to_str().expect("utf8");
        let pairs = [("HOME", "/home/u"), ("XDG_CONFIG_HOME", cfg)];
        let none = env_of(&pairs);
        assert_eq!(push_dir(&none), Some("/home/u/Downloads/fsync".into()));
        std::fs::write(
            dir.path().join("user-dirs.dirs"),
            "# c\nXDG_DOWNLOAD_DIR=\"$HOME/Herunterladen\"\n",
        )
        .expect("write");
        assert_eq!(push_dir(&none), Some("/home/u/Herunterladen/fsync".into()));
        let p2 = [("HOME", "/home/u"), ("XDG_DOWNLOAD_DIR", "/data/dl")];
        let explicit = env_of(&p2);
        assert_eq!(push_dir(&explicit), Some("/data/dl/fsync".into()));
        let p3 = [("HOME", "/home/u"), ("XDG_DOWNLOAD_DIR", "dl")];
        let relative = env_of(&p3);
        assert_eq!(push_dir(&relative), Some("/home/u/Downloads/fsync".into()));
        assert_eq!(push_dir(&env_of(&[])), None);
    }
}
