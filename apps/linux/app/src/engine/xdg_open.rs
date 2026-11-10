//! Opens a folder in the user's file manager through `xdg-open` (no D-Bus FileManager1: it is
//! not on every desktop). The child is reaped on its own thread.

use std::path::Path;
use std::process::{Command, Stdio};

/// False when `xdg-open` cannot be started.
pub fn open_dir(dir: &Path) -> bool {
    let child = Command::new("xdg-open")
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match child {
        Ok(mut c) => {
            let spawned = std::thread::Builder::new()
                .name("fsync-xdg-open".into())
                .spawn(move || {
                    let _ = c.wait();
                });
            if let Err(e) = spawned {
                tracing::warn!("cannot reap xdg-open: {e}");
            }
            true
        }
        Err(e) => {
            tracing::warn!("xdg-open could not start: {e}");
            false
        }
    }
}
