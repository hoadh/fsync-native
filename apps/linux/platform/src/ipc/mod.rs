//! Single-instance command channel: a Unix socket in a verified private directory.
//! Path: `<runtime_dir>/<blake3(canonical data dir)[..16]>.sock`.

pub mod client;
pub mod protocol;
#[cfg(feature = "e2e-control")]
mod protocol_e2e;
mod protocol_reply;
pub mod runtime_dir_check;
pub mod server;

pub use client::forward;
#[cfg(feature = "e2e-control")]
pub use protocol::E2eCommand;
pub use protocol::{IpcCommand, IpcRequest, MAX_LINE_BYTES, MAX_PATHS, ProtocolError, Reply};
pub use runtime_dir_check::{RuntimeDirError, ensure_runtime_dir};
pub use server::IpcServer;

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error(transparent)]
    RuntimeDir(#[from] RuntimeDirError),
    #[error("socket path unusable: {0}")]
    Socket(String),
    #[error("no running instance answers on the socket")]
    NoServer,
    #[error("ipc io: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
}

/// Socket location for a data dir (`data_dir_hash` = `paths::data_dir_hash`, 64 hex chars).
pub fn socket_path(runtime_dir: &Path, data_dir_hash: &str) -> PathBuf {
    let short = &data_dir_hash[..data_dir_hash.len().min(16)];
    runtime_dir.join(format!("{short}.sock"))
}

/// `sockaddr_un.sun_path` holds 108 bytes including the trailing NUL.
const MAX_SOCKET_PATH_BYTES: usize = 107;

/// `socket_path`, refused up front when the kernel would reject it (a long `XDG_RUNTIME_DIR`),
/// so users get a clear message instead of a raw `bind`/`connect` error.
pub fn checked_socket_path(runtime_dir: &Path, data_dir_hash: &str) -> Result<PathBuf, IpcError> {
    use std::os::unix::ffi::OsStrExt;
    let path = socket_path(runtime_dir, data_dir_hash);
    let len = path.as_os_str().as_bytes().len();
    if len > MAX_SOCKET_PATH_BYTES {
        return Err(IpcError::Socket(format!(
            "the socket path is {len} bytes long (limit {MAX_SOCKET_PATH_BYTES}): use a shorter \
             XDG_RUNTIME_DIR ({})",
            runtime_dir.display()
        )));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlong_socket_path_is_refused_with_a_clear_message() {
        let hash = "a".repeat(64);
        assert!(checked_socket_path(Path::new("/run/user/1000/fsync"), &hash).is_ok());
        let long = format!("/tmp/{}", "d".repeat(100));
        let err = checked_socket_path(Path::new(&long), &hash).unwrap_err();
        assert!(err.to_string().contains("shorter XDG_RUNTIME_DIR"), "{err}");
    }
}
