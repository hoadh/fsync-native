//! `FsyncService`: everything an app needs behind one object (identity, trust, node, backend
//! client, presence, history, engine, settings). The FFI crate is a thin adapter over this.
//!
//! Methods are blocking and may be called from any thread that is not inside a tokio runtime
//! (not a runtime worker, not within `block_on`), which includes the callback thread. Called
//! from async code they return `FsyncError::Internal` instead of panicking. Dropping the
//! service inside a runtime is safe too: teardown then runs on a helper thread. Background
//! work runs on an owned tokio runtime.

mod account;
mod clips;
mod config;
mod convert;
mod delegate;
mod devices;
mod engine_release;
mod error;
mod events;
mod history_api;
mod history_reps;
mod inner;
mod lifecycle;
mod lingering_hold;
mod login;
mod pairing;
mod pairing_control;
mod pairing_events;
mod presence;
mod revoke;
mod revoke_session;
#[cfg(test)]
mod stop_leak_tests;
mod transfers;
mod transfers_inner;
mod trust_reset;
mod types;
mod types_history;
mod walk;

pub use crate::settings::{ClipMode, Settings};
pub use config::{ServiceConfig, key32};
pub use delegate::ServiceDelegate;
pub use error::{FsyncError, SvcResult};
pub use revoke_session::{REVOKE_SESSION_BUDGET, revoke_session};
pub use types::*;
pub use types_history::*;

use inner::Inner;
use std::{future::Future, sync::Arc};

pub struct FsyncService {
    inner: Arc<Inner>,
    rt: Option<tokio::runtime::Runtime>,
    pump: tokio::task::JoinHandle<()>,
}

impl FsyncService {
    /// Opens the encrypted history and blob store and starts event delivery. Nothing touches
    /// the network until `start`.
    pub fn new(cfg: ServiceConfig, delegate: Arc<dyn ServiceDelegate>) -> SvcResult<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .thread_name("fsync-rt")
            .enable_all()
            .build()
            .map_err(|e| FsyncError::Internal(format!("runtime: {e}")))?;
        let (inner, events) = Inner::open(cfg, delegate)?;
        let inner = Arc::new(inner);
        let pump = rt.spawn(events::pump(inner.clone(), events));
        Ok(Self {
            inner,
            rt: Some(rt),
            pump,
        })
    }

    /// Abandons the history in `data_dir` after `new` failed with `HistoryKeyMismatch` (or the
    /// user wants a clean start): the files are moved aside (`history.corrupt-*`, nothing is
    /// deleted) and the key tag is cleared, so the next `new` with the current key succeeds.
    /// This also drops the pinned trust held in the history database: the device must re-pair
    /// (or go through trust reset). Call only while no service uses `data_dir`.
    pub fn reset_history(data_dir: &std::path::Path) -> SvcResult<()> {
        crate::history::reset_history(data_dir).map_err(FsyncError::from)
    }

    /// Runs `f` with the runtime entered (for code that calls `tokio::spawn`).
    pub(super) fn rt_enter<T>(&self, f: impl FnOnce() -> SvcResult<T>) -> SvcResult<T> {
        match &self.rt {
            Some(rt) => {
                let _g = rt.enter();
                f()
            }
            None => Err(FsyncError::Cancelled),
        }
    }

    pub(super) fn run<T>(&self, fut: impl Future<Output = SvcResult<T>>) -> SvcResult<T> {
        if tokio::runtime::Handle::try_current().is_ok() {
            // `block_on` would panic here; make it a plain error.
            return Err(FsyncError::Internal(
                "blocking service call made from inside an async runtime".into(),
            ));
        }
        match &self.rt {
            Some(rt) => rt.block_on(fut),
            None => Err(FsyncError::Cancelled),
        }
    }
}

/// Stops the node and shuts the runtime down; must not run inside another runtime.
fn teardown(rt: tokio::runtime::Runtime, inner: Arc<Inner>, pump: tokio::task::AbortHandle) {
    // Best effort: close the node cleanly before the runtime goes away.
    let _ = rt.block_on(async {
        tokio::time::timeout(engine_release::TEARDOWN_STOP_TIMEOUT, inner.stop()).await
    });
    pump.abort();
    rt.shutdown_timeout(std::time::Duration::from_secs(2));
}

impl Drop for FsyncService {
    fn drop(&mut self) {
        let Some(rt) = self.rt.take() else { return };
        let (inner, pump) = (self.inner.clone(), self.pump.abort_handle());
        if tokio::runtime::Handle::try_current().is_ok() {
            // Dropped from async code: blocking here would panic, so tear down on a thread.
            if let Err(e) = std::thread::Builder::new()
                .name("fsync-teardown".into())
                .spawn(move || teardown(rt, inner, pump))
            {
                tracing::warn!("could not start teardown thread: {e}");
            }
        } else {
            teardown(rt, inner, pump);
        }
    }
}
