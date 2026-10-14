//! Plain-directory blob store: `<root>/<hex>` complete, `<root>/<hex>.part` partial.

use super::{BlobInfo, BlobStore, Hash};
use crate::{
    error::{CoreError, Result},
    fs_util::{available_space, pread, random_suffix},
};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
    time::{Duration, SystemTime},
};

pub struct FsBlobStore {
    root: PathBuf,
    /// Serializes partial-file mutations: the offset check + append must be one step.
    part_lock: Mutex<()>,
}

impl FsBlobStore {
    /// Create (0700) and open the store directory.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self {
            root,
            part_lock: Mutex::new(()),
        })
    }

    fn path(&self, h: &Hash) -> PathBuf {
        self.root
            .join(blake3::Hash::from_bytes(*h).to_hex().as_str())
    }

    fn lock_parts(&self) -> Result<MutexGuard<'_, ()>> {
        self.part_lock.lock().map_err(|_| CoreError::Closed)
    }

    fn part(&self, h: &Hash) -> PathBuf {
        let mut p = self.path(h).into_os_string();
        p.push(".part");
        p.into()
    }
}

fn missing_ok<T: Default>(r: std::io::Result<T>) -> Result<T> {
    match r {
        Ok(v) => Ok(v),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e.into()),
    }
}

impl BlobStore for FsBlobStore {
    fn size_of(&self, h: &Hash) -> Result<Option<u64>> {
        match fs::metadata(self.path(h)) {
            Ok(m) => Ok(Some(m.len())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn read_at(&self, h: &Hash, offset: u64, buf: &mut [u8]) -> Result<usize> {
        Ok(pread(&File::open(self.path(h))?, buf, offset)?)
    }

    fn read_part_at(&self, h: &Hash, offset: u64, buf: &mut [u8]) -> Result<usize> {
        Ok(pread(&File::open(self.part(h))?, buf, offset)?)
    }

    fn put(&self, h: &Hash, data: &[u8]) -> Result<()> {
        if self.size_of(h)?.is_some() {
            return Ok(());
        }
        // Own unique temp file: never shares `<hash>.part` with a concurrent download.
        let tmp = self.root.join(format!(
            "{}.{}.part",
            blake3::Hash::from_bytes(*h).to_hex(),
            random_suffix()?
        ));
        let res = (|| -> Result<()> {
            let mut f = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
            f.write_all(data)?;
            f.sync_all()?;
            drop(f); // close before renaming (Windows)
            Ok(fs::rename(&tmp, self.path(h))?)
        })();
        if res.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        res
    }

    fn part_len(&self, h: &Hash) -> Result<u64> {
        missing_ok(fs::metadata(self.part(h)).map(|m| m.len()))
    }

    fn append_part(&self, h: &Hash, offset: u64, data: &[u8]) -> Result<()> {
        let _g = self.lock_parts()?;
        if self.part_len(h)? != offset {
            return Err(CoreError::Invalid("partial blob offset mismatch"));
        }
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.part(h))?;
        Ok(f.write_all(data)?)
    }

    fn truncate_part(&self, h: &Hash, len: u64) -> Result<()> {
        let _g = self.lock_parts()?;
        if len == 0 {
            return missing_ok(fs::remove_file(self.part(h)));
        }
        let f = OpenOptions::new().write(true).open(self.part(h))?;
        // Only shrinks: `set_len` would zero-extend and corrupt the resume state.
        if len > f.metadata()?.len() {
            return Err(CoreError::Invalid("cannot extend a partial blob"));
        }
        Ok(f.set_len(len)?)
    }

    fn finish(&self, h: &Hash, expected_size: u64) -> Result<()> {
        let _g = self.lock_parts()?;
        // A missing partial is an error (NotFound), never an empty blob. Opened for writing:
        // on Windows sync_all (FlushFileBuffers) needs write access.
        let f = OpenOptions::new().write(true).open(self.part(h))?;
        if f.metadata()?.len() != expected_size {
            return Err(CoreError::Invalid("partial blob length mismatch"));
        }
        f.sync_all()?;
        drop(f); // close before renaming (Windows)
        Ok(fs::rename(self.part(h), self.path(h))?)
    }

    fn remove(&self, h: &Hash) -> Result<()> {
        missing_ok(fs::remove_file(self.path(h)))?;
        missing_ok(fs::remove_file(self.part(h)))
    }

    fn free_space(&self) -> Result<Option<u64>> {
        available_space(&self.root).map(Some)
    }

    fn list(&self) -> Result<Vec<BlobInfo>> {
        let mut out = vec![];
        for e in fs::read_dir(&self.root)? {
            let e = e?;
            let name = e.file_name();
            let Some(hash) = name
                .to_str()
                .and_then(|n| blake3::Hash::from_hex(n).ok())
                .map(|h| *h.as_bytes())
            else {
                continue; // `.part` files and strangers
            };
            let m = e.metadata()?;
            out.push(BlobInfo {
                hash,
                size: m.len(),
                modified: m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            });
        }
        Ok(out)
    }

    fn sweep_stale_parts(&self, older_than: Duration) -> Result<usize> {
        let mut n = 0;
        for e in fs::read_dir(&self.root)? {
            let e = e?;
            if e.path().extension().is_none_or(|x| x != "part") {
                continue;
            }
            let age = e
                .metadata()?
                .modified()
                .ok()
                .and_then(|m| m.elapsed().ok())
                .unwrap_or_default();
            if age >= older_than && missing_ok(fs::remove_file(e.path())).is_ok() {
                n += 1;
            }
        }
        Ok(n)
    }
}
