//! Rust side of `cpp/clip_access.cpp`: QClipboard through a few plain functions. GUI thread only.

use fsync_linux_platform::clipboard::{ChangeReader, OwnedEntry, ReadError};
use std::time::Duration;

#[cxx::bridge(namespace = "fsync_clip")]
mod ffi {
    unsafe extern "C++" {
        include!("fsync-linux/cpp/clip_access.h");
        fn formats() -> Vec<String>;
        fn data(mime: &str) -> Vec<u8>;
        fn begin_set();
        fn add(mime: &str, bytes: &[u8]);
        fn commit_set() -> bool;
        fn settle(ms: i32);
    }
}

/// MIME names offered by the current clipboard owner.
pub fn formats() -> Vec<String> {
    ffi::formats()
}

/// Reads through QClipboard. It must only be called on the GUI thread (the QuickClipWindow
/// path calls `SyncEngine::collect_local` there); the type itself holds no state.
pub struct QtReader;

impl ChangeReader for QtReader {
    fn read(&self, mime: &str, cap: usize, _timeout: Duration) -> Result<Vec<u8>, ReadError> {
        // Qt applies its own owner timeout; the blocking read is bounded by it.
        let bytes = ffi::data(mime);
        if bytes.len() > cap {
            return Err(ReadError::TooLarge);
        }
        if bytes.is_empty() {
            return Err(ReadError::Stale);
        }
        Ok(bytes)
    }
}

/// Replaces the clipboard with `entry` (every offered name carries its rep's bytes).
pub fn set_entry(entry: &OwnedEntry) -> bool {
    ffi::begin_set();
    for rep in &entry.reps {
        for name in &rep.offered {
            // Qt labels STRING / TEXT as Latin-1 on its X11 platform but our bytes are UTF-8;
            // Qt derives its own text aliases from text/plain.
            if name == "STRING" || name == "TEXT" {
                continue;
            }
            ffi::add(name, &rep.data);
        }
    }
    ffi::commit_set()
}

/// Lets Qt process events for `ms` (compositor round trip after a clipboard write).
pub fn settle(ms: i32) {
    ffi::settle(ms);
}
