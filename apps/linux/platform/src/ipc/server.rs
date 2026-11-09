//! IPC server. Only the `app.lock` holder may call `start` (proved by `&InstanceLock`), so it
//! alone may unlink a stale socket and bind.

use super::protocol::{IpcRequest, Reply, read_line_bounded};
use super::{IpcError, checked_socket_path, ensure_runtime_dir};
use crate::instance_lock::InstanceLock;
use rustix::event::{EventfdFlags, PollFd, PollFlags, eventfd, poll};
use rustix::io::Errno;
use rustix::process::getuid;
use std::io::{BufReader, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

const IO_TIMEOUT: Duration = Duration::from_secs(2);

/// Called on the server thread with a validated request; its `Reply` is written back.
pub type Handler = dyn Fn(IpcRequest) -> Reply + Send + Sync;

pub struct IpcServer {
    path: PathBuf,
    wake: OwnedFd,
    thread: Option<JoinHandle<()>>,
}

/// Remove a leftover socket from a crashed run; anything else at that path is an error.
fn unlink_stale(path: &Path) -> Result<(), IpcError> {
    match path.symlink_metadata() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
        Ok(m) if m.file_type().is_socket() && m.uid() == getuid().as_raw() => {
            std::fs::remove_file(path).map_err(Into::into)
        }
        Ok(_) => Err(IpcError::Socket(format!(
            "{} exists and is not our socket",
            path.display()
        ))),
    }
}

fn serve_one(stream: UnixStream, handler: &Handler) {
    let peer_ok = rustix::net::sockopt::socket_peercred(&stream).is_ok_and(|c| c.uid == getuid());
    if !peer_ok {
        tracing::warn!("ipc: dropped connection from another user");
        return;
    }
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let reply = match stream.try_clone() {
        Err(e) => Reply::err(e.to_string()),
        Ok(rd) => match read_line_bounded(&mut BufReader::new(rd))
            .and_then(|l| IpcRequest::parse_line(&l))
        {
            Ok(req) => handler(req),
            Err(e) => Reply::err(e.to_string()),
        },
    };
    let mut out = reply.to_line();
    out.push('\n');
    if let Err(e) = (&stream).write_all(out.as_bytes()) {
        tracing::debug!("ipc: reply not delivered: {e}");
    }
}

/// Serves connections until `wake` becomes readable.
fn accept_loop(listener: &UnixListener, wake: &OwnedFd, handler: &Handler) {
    loop {
        let mut fds = [
            PollFd::new(listener, PollFlags::IN),
            PollFd::new(wake, PollFlags::IN),
        ];
        match poll(&mut fds, None) {
            Ok(_) => {}
            Err(Errno::INTR) => continue,
            Err(e) => {
                tracing::error!("ipc poll failed, server thread ends: {e}");
                return;
            }
        }
        if !fds[1].revents().is_empty() {
            return;
        }
        if fds[0].revents().is_empty() {
            continue;
        }
        match listener.accept() {
            Ok((s, _)) => {
                // Accepted streams must block (bounded by the IO timeouts).
                if s.set_nonblocking(false).is_ok() {
                    serve_one(s, handler);
                }
            }
            // Connection gone between poll and accept.
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => tracing::warn!("ipc accept: {e}"),
        }
    }
}

impl IpcServer {
    pub fn start(
        runtime_dir: &Path,
        data_dir_hash: &str,
        _lock: &InstanceLock,
        handler: Arc<Handler>,
    ) -> Result<Self, IpcError> {
        ensure_runtime_dir(runtime_dir)?;
        let path = checked_socket_path(runtime_dir, data_dir_hash)?;
        unlink_stale(&path)?;
        let listener = UnixListener::bind(&path)
            .map_err(|e| IpcError::Socket(format!("bind {}: {e}", path.display())))?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        // `stop` writes the eventfd; the thread sleeps in poll() on listener + eventfd with no
        // timeout (zero idle wake-ups), and `stop` works even if the socket file is gone.
        let wake = eventfd(0, EventfdFlags::CLOEXEC).map_err(std::io::Error::from)?;
        let wake_for_thread = wake.try_clone()?;
        let thread = std::thread::Builder::new()
            .name("fsync-ipc".into())
            .spawn(move || accept_loop(&listener, &wake_for_thread, handler.as_ref()))?;
        Ok(Self {
            path,
            wake,
            thread: Some(thread),
        })
    }

    pub fn socket(&self) -> &Path {
        &self.path
    }

    /// Stop accepting, join the thread, unlink the socket.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        let Some(t) = self.thread.take() else { return };
        if let Err(e) = rustix::io::write(&self.wake, &1u64.to_ne_bytes()) {
            tracing::warn!("ipc: cannot wake the accept loop: {e}");
        }
        let _ = t.join();
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}
