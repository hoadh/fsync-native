//! IPC client: forward a request to the running instance and wait for its reply.

use super::protocol::{IpcRequest, Reply, read_line_bounded};
use super::{IpcError, checked_socket_path, ensure_runtime_dir};
use crate::instance_lock::InstanceLock;
use std::io::{BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

const RETRIES: u32 = 20;
const RETRY_DELAY: Duration = Duration::from_millis(100);
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// One connect + request + reply on `socket`.
pub fn send_once(
    socket: &Path,
    req: &IpcRequest,
    reply_timeout: Duration,
) -> Result<Reply, IpcError> {
    let stream = UnixStream::connect(socket)?;
    stream.set_write_timeout(Some(reply_timeout))?;
    stream.set_read_timeout(Some(reply_timeout))?;
    let mut line = req.to_line()?;
    line.push('\n');
    (&stream).write_all(line.as_bytes())?;
    let reply = read_line_bounded(&mut BufReader::new(&stream))?;
    serde_json::from_str(reply.trim())
        .map_err(|e| super::ProtocolError::Invalid(format!("bad reply: {e}")).into())
}

/// Forward `req` to the instance that owns `data_dir`. While the lock is held but the socket does
/// not answer yet (holder still binding) retry 20 x 100 ms. The client never unlinks anything.
/// `NoServer`: nobody holds the lock or the holder never opened a socket.
pub fn forward(
    runtime_dir: &Path,
    data_dir: &Path,
    data_dir_hash: &str,
    req: &IpcRequest,
) -> Result<Reply, IpcError> {
    ensure_runtime_dir(runtime_dir)?;
    let socket = checked_socket_path(runtime_dir, data_dir_hash)?;
    for attempt in 0..RETRIES {
        match send_once(&socket, req, REPLY_TIMEOUT) {
            Err(IpcError::Io(e)) if attempt + 1 < RETRIES && is_connect_failure(&e) => {
                if !InstanceLock::is_held(data_dir) {
                    return Err(IpcError::NoServer);
                }
                std::thread::sleep(RETRY_DELAY);
            }
            Err(IpcError::Io(e)) if is_connect_failure(&e) => return Err(IpcError::NoServer),
            other => return other,
        }
    }
    Err(IpcError::NoServer)
}

fn is_connect_failure(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
    )
}
