//! Expands the paths an app hands over into the flat file list of an offer.

use super::error::{FsyncError, SvcResult};
use crate::clip::FileInput;
use fsync_proto::clip::MAX_FILES;
use std::{fs, path::Path};

const MAX_DEPTH: usize = 32;

/// Files stay top level; directories keep their structure under their own name. Symlinks are
/// skipped (they could point outside what the user picked).
pub(super) fn expand(paths: &[String]) -> SvcResult<Vec<FileInput>> {
    let mut out = Vec::new();
    for p in paths {
        let path = Path::new(p);
        let meta = fs::symlink_metadata(path).map_err(|e| FsyncError::Io(e.to_string()))?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .ok_or_else(|| FsyncError::invalid("directory has no name"))?;
            walk_dir(path, &name, 0, &mut out)?;
        } else if meta.is_file() {
            push(&mut out, FileInput::new(path))?;
        }
    }
    if out.is_empty() {
        return Err(FsyncError::invalid("nothing to send"));
    }
    Ok(out)
}

fn push(out: &mut Vec<FileInput>, f: FileInput) -> SvcResult<()> {
    if out.len() >= MAX_FILES {
        return Err(FsyncError::Invalid(format!("more than {MAX_FILES} files")));
    }
    out.push(f);
    Ok(())
}

fn walk_dir(dir: &Path, rel: &str, depth: usize, out: &mut Vec<FileInput>) -> SvcResult<()> {
    if depth > MAX_DEPTH {
        return Err(FsyncError::invalid("directory nesting too deep"));
    }
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| FsyncError::Io(e.to_string()))?
        .collect::<Result<_, _>>()
        .map_err(|e| FsyncError::Io(e.to_string()))?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let ft = e.file_type().map_err(|e| FsyncError::Io(e.to_string()))?;
        let name = e.file_name().to_string_lossy().into_owned();
        let child = format!("{rel}/{name}");
        if ft.is_dir() {
            walk_dir(&e.path(), &child, depth + 1, out)?;
        } else if ft.is_file() {
            push(
                out,
                FileInput {
                    path: e.path(),
                    rel_path: child,
                },
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_dirs_keeps_structure_and_skips_symlinks() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("photos");
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("a.txt"), b"a").unwrap();
        fs::write(root.join("sub/b.txt"), b"b").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/hosts", root.join("link")).unwrap();
        let single = d.path().join("one.bin");
        fs::write(&single, b"1").unwrap();
        let got = expand(&[
            root.to_string_lossy().into_owned(),
            single.to_string_lossy().into_owned(),
        ])
        .unwrap();
        let rels: Vec<_> = got.iter().map(|f| f.rel_path.as_str()).collect();
        assert_eq!(rels, ["photos/a.txt", "photos/sub/b.txt", "one.bin"]);
    }

    #[test]
    fn empty_and_missing_are_errors() {
        assert!(expand(&[]).is_err());
        assert!(expand(&["/definitely/not/here".into()]).is_err());
    }
}
