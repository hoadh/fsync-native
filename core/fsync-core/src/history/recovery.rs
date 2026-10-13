//! Key-loss and corruption handling for the history database files.
//!
//! SQLCipher reports a wrong key and a damaged file identically, so `history.keycheck` records
//! a keyed tag of the key that last opened the database. It tells the two apart.

use super::db::DB_FILE;
use crate::{error::Result, fs_util::write_private_atomic};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;

const KEYCHECK_FILE: &str = "history.keycheck";
const BACKUP_PREFIX: &str = "history.corrupt-";
/// Newest backups that are always kept.
const KEEP_BACKUPS: usize = 3;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum KeyVerdict {
    /// The recorded tag matches: the file itself is damaged.
    Same,
    /// The recorded tag belongs to another key.
    Mismatch,
    /// Nothing recorded (first run, or a database from before the tag existed).
    Unknown,
}

fn tag(key: &[u8; 32]) -> String {
    blake3::keyed_hash(key, b"fsync history key check v1")
        .to_hex()
        .to_string()
}

pub(super) fn key_verdict(dir: &Path, key: &[u8; 32]) -> KeyVerdict {
    match std::fs::read_to_string(dir.join(KEYCHECK_FILE)) {
        Ok(on_disk) if bool::from(on_disk.trim().as_bytes().ct_eq(tag(key).as_bytes())) => {
            KeyVerdict::Same
        }
        Ok(_) => KeyVerdict::Mismatch,
        Err(_) => KeyVerdict::Unknown,
    }
}

/// Remember which key opened the database. Best effort: without it a later mismatch is
/// treated like corruption, as before the tag existed.
pub(super) fn record_key(dir: &Path, key: &[u8; 32]) {
    if key_verdict(dir, key) == KeyVerdict::Same {
        return;
    }
    if let Err(e) = write_private_atomic(&dir.join(KEYCHECK_FILE), tag(key).as_bytes()) {
        tracing::warn!("could not record the history key check: {e}");
    }
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

/// Rename `history.db{,-wal,-shm}` to `history.corrupt-<ts>{,-wal,-shm}`.
pub(super) fn move_aside(dir: &Path) -> Result<PathBuf> {
    let ts = unix_ms();
    for suffix in ["", "-wal", "-shm"] {
        let from = dir.join(format!("{DB_FILE}{suffix}"));
        if from.exists() {
            std::fs::rename(&from, dir.join(format!("{BACKUP_PREFIX}{ts}{suffix}")))?;
        }
    }
    Ok(dir.join(format!("{BACKUP_PREFIX}{ts}")))
}

/// Explicit abandon: the user chose to start a new history (typically after
/// `HistoryKeyMismatch`). Moves the files aside (nothing is deleted) and forgets the key tag, so
/// the next open with any key creates a fresh database. It also drops the pinned trust stored in
/// the history. Call only while no service uses `dir`. A missing directory is not an error.
pub fn reset_history(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    move_aside(dir)?;
    match std::fs::remove_file(dir.join(KEYCHECK_FILE)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Size of a database file plus its WAL (recent data may only be in the WAL).
fn footprint(main: &Path) -> u64 {
    let len = |p: &Path| std::fs::metadata(p).map_or(0, |m| m.len());
    let wal = PathBuf::from(format!("{}-wal", main.display()));
    len(main) + len(&wal)
}

/// Drops old backups: the newest `KEEP_BACKUPS` stay, older ones only if they hold no more than
/// a freshly created database (`fresh` must be the empty database a move-aside just created;
/// never call this with a live one), i.e. no real history.
/// Best effort: a file that cannot be removed is left for next time.
pub(super) fn prune_backups(dir: &Path, fresh: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let empty_len = footprint(fresh);
    let mut stamps: Vec<u128> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let rest = name.strip_prefix(BACKUP_PREFIX)?;
            rest.split('-').next()?.parse().ok()
        })
        .collect();
    stamps.sort_unstable_by(|a, b| b.cmp(a));
    stamps.dedup();
    for ts in stamps.into_iter().skip(KEEP_BACKUPS) {
        let main = dir.join(format!("{BACKUP_PREFIX}{ts}"));
        if footprint(&main) > empty_len {
            continue; // has data (or cannot tell): never delete
        }
        for suffix in ["", "-wal", "-shm"] {
            let p = dir.join(format!("{BACKUP_PREFIX}{ts}{suffix}"));
            if let Err(e) = std::fs::remove_file(&p)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                tracing::debug!("could not remove old history backup {p:?}: {e}");
            }
        }
    }
}
