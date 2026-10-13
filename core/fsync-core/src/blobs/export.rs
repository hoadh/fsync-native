//! Safe placement of a blob's bytes into a user directory (no clobber, no symlink following).

use crate::{
    error::{CoreError, Result},
    fs_util::random_suffix,
};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

const MAX_COLLISIONS: u32 = 1000;
/// Longest file name component (bytes) on the platforms we support.
const NAME_MAX: usize = 255;

/// Removes the file unless disarmed (temp on error, reservation on failed rename).
struct Cleanup<'a>(&'a Path, bool);

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        if self.1 {
            let _ = fs::remove_file(self.0);
        }
    }
}

/// Stream `read(offset, buf)` (0 = EOF) into a new file at `dest`, or at `name (n).ext` if
/// `dest` is taken. Returns the final path.
pub fn export_with(
    dest: &Path,
    mut read: impl FnMut(u64, &mut [u8]) -> Result<usize>,
) -> Result<PathBuf> {
    let dir = dest.parent().ok_or(CoreError::Invalid("bad path"))?;
    let name = dest
        .file_name()
        .ok_or(CoreError::Invalid("bad path"))?
        .to_string_lossy()
        .into_owned();
    // `create_new` is O_EXCL: it fails on any existing entry, symlinks included.
    // The temp name is independent of `name`, so it fits NAME_MAX whatever the file is called.
    let tmp = dir.join(format!(".{}.fsync-part", random_suffix()?));
    let mut out = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
    let mut tmp_guard = Cleanup(&tmp, true);
    let mut buf = vec![0u8; 1 << 20];
    let mut off = 0u64;
    loop {
        let n = read(off, &mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        off += n as u64;
    }
    out.sync_all()?;
    drop(out);
    // Reserve the final name atomically, then rename over our own reservation. Rename replaces
    // a symlink itself rather than following it, so nothing outside `dir` is ever touched.
    for n in 0..=MAX_COLLISIONS {
        let cand = dir.join(candidate(&name, n));
        match OpenOptions::new().write(true).create_new(true).open(&cand) {
            Ok(f) => {
                drop(f);
                if let Err(e) = fs::rename(&tmp, &cand) {
                    let _ = fs::remove_file(&cand);
                    return Err(e.into());
                }
                tmp_guard.1 = false;
                return Ok(cand);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    Err(io::Error::from(io::ErrorKind::AlreadyExists).into())
}

/// `name`, `name (1).ext`, `name (2).ext`, ... Never longer than `NAME_MAX` bytes: the stem
/// gives way to the suffix.
fn candidate(name: &str, n: u32) -> String {
    if n == 0 {
        return name.to_owned();
    }
    let p = Path::new(name);
    let (stem, ext) = match (p.file_stem(), p.extension()) {
        (Some(stem), Some(ext)) => (
            stem.to_string_lossy().into_owned(),
            format!(".{}", ext.to_string_lossy()),
        ),
        _ => (name.to_owned(), String::new()),
    };
    let tail = format!(" ({n}){ext}");
    let mut keep = NAME_MAX.saturating_sub(tail.len()).min(stem.len());
    while !stem.is_char_boundary(keep) {
        keep -= 1;
    }
    format!("{}{tail}", &stem[..keep])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_names_stay_within_name_max() {
        assert_eq!(candidate("a.txt", 0), "a.txt");
        assert_eq!(candidate("a.txt", 2), "a (2).txt");
        assert_eq!(candidate("noext", 1), "noext (1)");
        let long = format!("{}.txt", "é".repeat(125));
        let c = candidate(&long, 12);
        assert!(
            c.len() <= NAME_MAX && c.ends_with(" (12).txt"),
            "{}",
            c.len()
        );
    }
}
