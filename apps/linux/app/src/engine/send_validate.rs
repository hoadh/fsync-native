//! Pure rules of "send to a device": refusal codes and sentences, path validation (the same
//! allowlist / denylist as clipboard file clips), drop URL decoding, the picker summary and the
//! cold-start decision. No Core, no Qt.

use crate::core_state::ui_state::EngineState;
use fsync_linux_platform::clipboard::path_denylist::{PathDenied, PathDenylist};
use fsync_linux_platform::clipboard::uri_list::parse_uri_list;
use fsync_linux_platform::ipc::MAX_PATHS;
use std::path::{Path, PathBuf};

pub const PROTECTED: &str = "path-protected";
pub const MISSING: &str = "path-missing";
pub const UNSUPPORTED: &str = "path-unsupported";
pub const NOT_LOCAL: &str = "not-local";
pub const NOTHING_STAGED: &str = "nothing-staged";
pub const TOO_MANY: &str = "too-many";
pub const STALE: &str = "stage-changed";
pub const SIGNED_OUT: &str = "signed-out";
pub const SHUTTING_DOWN: &str = "shutting-down";
pub const BUSY: &str = "busy";
/// A newer request replaced this one while it was being checked (never shown).
pub const SUPERSEDED: &str = "superseded";
pub const NEEDS_PAIRING: &str = "needs-pairing";
pub const KEYRING_LOCKED: &str = "keyring-locked";
pub const OFFLINE: &str = "offline";
pub const NOT_READY: &str = "not-ready";

/// Sentence for a refusal code (window message; the IPC reply carries the code).
pub fn refusal_text(code: &str) -> &'static str {
    match code {
        PROTECTED => {
            "Those files are protected (keys, keyrings, app data) or outside your home folder and removable media: not sent."
        }
        MISSING => "A file no longer exists.",
        UNSUPPORTED => "File names that are not UTF-8 cannot be sent yet.",
        NOT_LOCAL => "Only files on this computer can be sent.",
        NOTHING_STAGED => "There is nothing to send.",
        TOO_MANY => "Too many files at once: send fewer than 1000 items.",
        STALE => "The files changed: check the list and pick again.",
        SIGNED_OUT => "Sign in first, then send the files again.",
        SHUTTING_DOWN => "fsync is shutting down.",
        BUSY => "Still checking earlier files: try again in a moment.",
        NEEDS_PAIRING => "Pair a device first, then send the files again.",
        KEYRING_LOCKED => "Unlock the keyring, then send the files again.",
        OFFLINE => "fsync is offline: send the files again once it is connected.",
        NOT_READY => "fsync is not ready yet: send the files again in a moment.",
        _ => "The files could not be sent.",
    }
}

/// What a cold-start `--send` does in the engine state `state`.
#[derive(Debug, PartialEq, Eq)]
pub enum ColdStart {
    Stage,
    Wait,
    Refuse(&'static str),
}

/// `expired`: the wait budget is used up. Signed out and engine errors refuse at once (the
/// files are not kept across a sign-in); a state that may still resolve waits, and on expiry
/// the sentence names the real state.
pub fn cold_start(state: EngineState, expired: bool) -> ColdStart {
    match state {
        EngineState::Ready => ColdStart::Stage,
        EngineState::SignedOut | EngineState::AwaitingCode => ColdStart::Refuse(SIGNED_OUT),
        EngineState::Error => ColdStart::Refuse(NOT_READY),
        EngineState::NeedsPairing => ColdStart::Refuse(NEEDS_PAIRING),
        EngineState::WaitingForKeyring if expired => ColdStart::Refuse(KEYRING_LOCKED),
        EngineState::Offline if expired => ColdStart::Refuse(OFFLINE),
        _ if expired => ColdStart::Refuse(NOT_READY),
        _ => ColdStart::Wait,
    }
}

/// Canonical UTF-8 paths, or the refusal code of the first path that fails.
pub fn validate(deny: &PathDenylist, raw: &[PathBuf]) -> Result<Vec<String>, &'static str> {
    if raw.is_empty() {
        return Err(NOTHING_STAGED);
    }
    if raw.len() > MAX_PATHS {
        return Err(TOO_MANY);
    }
    raw.iter()
        .map(|p| {
            let canon = deny.check(p).map_err(|e| match e {
                PathDenied::Unresolvable => MISSING,
                PathDenied::Protected => PROTECTED,
            })?;
            // Core takes `String`s: a name that is not UTF-8 would be altered on the way.
            canon
                .into_os_string()
                .into_string()
                .map_err(|_| UNSUPPORTED)
        })
        .collect()
}

/// Percent-encoded `file://` URLs from a drop, one per line, to local paths (decoded once, by
/// `parse_uri_list`). Anything that is not a local file URL refuses the whole drop: a partial
/// send would surprise.
pub fn urls_to_paths(text: &str) -> Result<Vec<PathBuf>, &'static str> {
    // Encoded URLs never hold a raw space or newline, and no name is trimmed.
    let urls: Vec<&str> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .filter(|l| !l.is_empty())
        .collect();
    let paths = parse_uri_list(urls.join("\r\n").as_bytes(), false);
    if paths.is_empty() || paths.len() != urls.len() {
        return Err(NOT_LOCAL);
    }
    Ok(paths)
}

fn size_text(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KIB * KIB {
        format!("{:.1} KiB", b / KIB)
    } else if b < KIB * KIB * KIB {
        format!("{:.1} MiB", b / (KIB * KIB))
    } else {
        format!("{:.2} GiB", b / (KIB * KIB * KIB))
    }
}

/// What the picker shows. `meta` answers `Some(size)` for a file, `None` for a folder (its
/// content is only counted by core when it sends).
pub fn summarize(paths: &[String], meta: impl Fn(&Path) -> Option<u64>) -> String {
    let (mut total, mut folders) = (0u64, 0usize);
    let names: Vec<String> = paths
        .iter()
        .map(|p| {
            let path = Path::new(p);
            match meta(path) {
                Some(n) => total += n,
                None => folders += 1,
            }
            path.file_name()
                .map_or_else(|| p.clone(), |n| n.to_string_lossy().into_owned())
        })
        .collect();
    let n = names.len();
    let mut out = format!("{n} item{}: {}", if n == 1 { "" } else { "s" }, {
        let shown: Vec<&str> = names.iter().take(3).map(String::as_str).collect();
        shown.join(", ")
    });
    if n > 3 {
        out.push_str(&format!(" and {} more", n - 3));
    }
    if n > folders {
        out.push_str(&format!(" ({}", size_text(total)));
        if folders > 0 {
            out.push_str(&format!(
                ", plus {folders} folder{}",
                if folders == 1 { "" } else { "s" }
            ));
        }
        out.push(')');
    } else {
        out.push_str(" (folders)");
    }
    out
}
