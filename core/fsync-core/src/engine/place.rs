//! Placing a fetched file into a user-chosen directory.

use super::Engine;
use crate::file::check_declared_size;
use crate::{
    blobs::blocking,
    error::{CoreError, Result},
};
use fsync_proto::names::validate_rel_path;
use std::path::{Path, PathBuf};
use uuid::Uuid;

impl Engine {
    /// Pull a file and place it at `dest_dir/<rel_path>`. Never overwrites (a taken name becomes
    /// `name (1).ext`; the final path is returned) and refuses any path that would land outside
    /// `dest_dir`, including through symlinks.
    pub async fn fetch_file_to(
        &self,
        item_id: Uuid,
        file_id: u32,
        dest_dir: &Path,
    ) -> Result<PathBuf> {
        let r = self.sh.offer_of(&item_id)?;
        let f = r
            .offer
            .files
            .iter()
            .find(|f| f.file_id == file_id)
            .ok_or(CoreError::Invalid("no such file"))?;
        validate_rel_path(&f.rel_path)?;
        let (rel, hash, size) = (f.rel_path.clone(), f.hash, f.size);
        let dir = dest_dir.to_owned();
        // The copy needs room at the destination too.
        let free = blocking(move || crate::fs_util::available_space(&dir)).await?;
        check_declared_size(size, size, self.sh.cfg.max_file_bytes, Some(free))?;
        self.fetch_file(item_id, file_id).await?;
        let (store, dir) = (self.sh.store.clone(), dest_dir.to_owned());
        let dest = blocking(move || {
            let root = dir.canonicalize()?;
            let parent = create_dirs_in(&root, &rel)?;
            let name = rel.rsplit('/').next().unwrap_or_default();
            // Never overwrites: a taken name gets `name (1).ext`.
            store.export(&hash, &parent.join(name))
        })
        .await?;
        if r.offer.sensitive {
            self.drop_blob(hash).await;
        }
        self.sh.note_file_saved(item_id, file_id, &dest).await;
        Ok(dest)
    }
}

impl Engine {
    /// Remove a fetched blob that must not linger (sensitive items), unless a history item
    /// or a live offer still uses the same content. Failure is only logged.
    pub(super) async fn drop_blob(&self, hash: [u8; 32]) {
        let sh = self.sh.clone();
        blocking(move || {
            sh.remove_blobs(vec![hash]);
            Ok(())
        })
        .await
        .ok();
    }
}

/// Create the parent directories of `rel` under the canonical `root`, one component at a time,
/// refusing symlinks and non-directories. Containment is checked lexically before anything is
/// created; a refusal leaves no directories behind.
fn create_dirs_in(root: &Path, rel: &str) -> Result<PathBuf> {
    let comps: Vec<&str> = rel.split('/').collect();
    let dirs = &comps[..comps.len().saturating_sub(1)];
    let planned = dirs.iter().fold(root.to_owned(), |p, c| p.join(c));
    if dirs.iter().any(|c| matches!(*c, "" | "." | "..")) || !planned.starts_with(root) {
        return Err(CoreError::Invalid(
            "destination escapes the target directory",
        ));
    }
    let mut cur = root.to_owned();
    for c in dirs {
        cur.push(c);
        match std::fs::symlink_metadata(&cur) {
            Ok(m) if m.is_dir() => {}
            Ok(_) => {
                return Err(CoreError::Invalid(
                    "path component is a link or not a directory",
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                match std::fs::create_dir(&cur) {
                    Ok(()) => {}
                    // Lost a race: only fine if the winner made a real directory.
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                        if !std::fs::symlink_metadata(&cur)?.is_dir() {
                            return Err(CoreError::Invalid("path component is a link"));
                        }
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(cur)
}
