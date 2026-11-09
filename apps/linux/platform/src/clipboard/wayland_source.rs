//! Pipe I/O for data-control transfers, always bounded: reads stop at a size cap or a deadline,
//! writes stop when the receiver stalls or the backend shuts down. Serving never runs on the
//! Wayland dispatch thread, so one stalled paste cannot block cancellation or shutdown.

use super::{ReadError, wayland_watch::WlState};
use rustix::event::{PollFd, PollFlags};
use rustix::fs::{OFlags, fcntl_setfl};
use rustix::pipe::{PipeFlags, pipe_with};
use rustix::time::Timespec;
use std::os::fd::{AsFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// A paste that makes no progress for this long is abandoned.
pub const WRITE_IDLE: Duration = Duration::from_secs(5);
/// Poll slice: the stop flag is checked this often.
const SLICE: Duration = Duration::from_millis(100);
const MAX_WRITERS: usize = 16;

pub fn make_pipe() -> std::io::Result<(OwnedFd, OwnedFd)> {
    pipe_with(PipeFlags::CLOEXEC).map_err(std::io::Error::from)
}

fn ts(d: Duration) -> Timespec {
    Timespec {
        tv_sec: d.as_secs() as _,
        tv_nsec: d.subsec_nanos() as _,
    }
}

fn io_err(e: impl Into<std::io::Error>) -> ReadError {
    ReadError::Io(e.into().kind().to_string())
}

/// Reads the pipe to EOF (the data source closed it). `cap` bounds the bytes, `timeout` the
/// whole read.
pub fn read_pipe(fd: OwnedFd, cap: usize, timeout: Duration) -> Result<Vec<u8>, ReadError> {
    fcntl_setfl(&fd, OFlags::NONBLOCK).map_err(io_err)?;
    let deadline = Instant::now() + timeout;
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let left = deadline
            .checked_duration_since(Instant::now())
            .ok_or(ReadError::Timeout)?;
        let mut fds = [PollFd::new(&fd, PollFlags::IN)];
        match rustix::event::poll(&mut fds, Some(&ts(left))) {
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(e) => return Err(io_err(e)),
        }
        match rustix::io::read(&fd, &mut buf) {
            Ok(0) => return Ok(out),
            Ok(n) => {
                if out.len() + n > cap {
                    return Err(ReadError::TooLarge);
                }
                out.extend_from_slice(&buf[..n]);
            }
            Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => {}
            Err(e) => return Err(io_err(e)),
        }
    }
}

/// Writes all of `data`; `Ok(false)` when stopped (shutdown) or the receiver stalled for
/// `idle`. A closed pipe (the paster read enough) counts as done.
pub fn write_bounded(
    fd: &OwnedFd,
    data: &[u8],
    stop: &AtomicBool,
    idle: Duration,
) -> std::io::Result<bool> {
    fcntl_setfl(fd, OFlags::NONBLOCK).map_err(std::io::Error::from)?;
    let mut sent = 0;
    let mut last_progress = Instant::now();
    while sent < data.len() {
        if stop.load(Ordering::SeqCst) || last_progress.elapsed() > idle {
            return Ok(false);
        }
        let mut fds = [PollFd::new(fd, PollFlags::OUT)];
        match rustix::event::poll(&mut fds, Some(&ts(SLICE))) {
            Ok(_) | Err(rustix::io::Errno::INTR) => {}
            Err(e) => return Err(e.into()),
        }
        if fds[0].revents().contains(PollFlags::ERR) {
            return Ok(true); // reader closed
        }
        match rustix::io::write(fd.as_fd(), &data[sent..]) {
            Ok(n) => {
                sent += n;
                last_progress = Instant::now();
            }
            Err(rustix::io::Errno::AGAIN | rustix::io::Errno::INTR) => {}
            Err(rustix::io::Errno::PIPE) => return Ok(true),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(true)
}

/// Counts running writer threads so shutdown can wait for them (bounded) and a flood of paste
/// requests cannot spawn unbounded threads.
#[derive(Default)]
pub struct WriterGate {
    count: Mutex<usize>,
    idle: Condvar,
}

pub struct WriterSlot(Arc<WriterGate>);

impl Drop for WriterSlot {
    fn drop(&mut self) {
        let mut c = self.0.count.lock().unwrap_or_else(|p| p.into_inner());
        *c -= 1;
        self.0.idle.notify_all();
    }
}

impl WriterGate {
    pub fn enter(self: &Arc<Self>) -> Option<WriterSlot> {
        let mut c = self.count.lock().unwrap_or_else(|p| p.into_inner());
        if *c >= MAX_WRITERS {
            return None;
        }
        *c += 1;
        Some(WriterSlot(Arc::clone(self)))
    }

    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut c = self.count.lock().unwrap_or_else(|p| p.into_inner());
        while *c > 0 {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            c = self
                .idle
                .wait_timeout(c, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        true
    }
}

impl WlState {
    /// A paste request for our source: serve it on its own short-lived thread.
    pub(super) fn serve_request(&self, rep: usize, fd: OwnedFd) {
        let Some(entry) = self.entry.clone() else {
            return;
        };
        let Some(slot) = self.writers.enter() else {
            tracing::warn!("clipboard: too many concurrent paste requests, one refused");
            return;
        };
        let stop = Arc::clone(&self.stop);
        let spawned = std::thread::Builder::new()
            .name("fsync-wl-serve".into())
            .spawn(move || {
                let _slot = slot;
                match write_bounded(&fd, &entry.reps[rep].data, &stop, WRITE_IDLE) {
                    Ok(true) => {}
                    Ok(false) => tracing::debug!("clipboard serve abandoned (stalled or stopping)"),
                    Err(e) => tracing::debug!("clipboard serve failed: {}", e.kind()),
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("cannot start clipboard serve thread: {e}");
        }
    }
}

#[cfg(test)]
#[path = "wayland_source_tests.rs"]
mod tests;
