//! Callbacks to the app. They run on one dedicated thread (never a runtime worker), in event
//! order, so a callback may call blocking service methods; a slow callback only delays later
//! callbacks.

use super::{error::FsyncError, types::*};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        mpsc::{SyncSender, sync_channel},
    },
    thread::JoinHandle,
};

pub trait ServiceDelegate: Send + Sync + 'static {
    fn on_remote_clip(&self, item: ClipItem);
    fn on_device_status(&self, device: Device);
    fn on_transfer(&self, event: TransferEvent);
    fn on_incoming_files(&self, offer: ClipItem);
    fn on_pairing_event(&self, event: PairingEvent);
    fn on_account_event(&self, event: AccountEvent);
    /// Tokens were refreshed: persist them (Keychain / Credential Locker).
    fn on_session_changed(&self, tokens: SessionTokens);
    fn on_error(&self, error: FsyncError);
}

pub(super) enum Out {
    Clip(ClipItem),
    Device(Device),
    Transfer(TransferEvent),
    Files(ClipItem),
    Pairing(PairingEvent),
    Account(AccountEvent),
    Session(SessionTokens),
    Error(FsyncError),
}

const QUEUE: usize = 4096;

pub(super) struct Dispatcher {
    tx: Option<SyncSender<Out>>,
    thread: Option<JoinHandle<()>>,
}

impl Dispatcher {
    pub(super) fn spawn(d: Arc<dyn ServiceDelegate>) -> Self {
        let (tx, rx) = sync_channel::<Out>(QUEUE);
        let thread = std::thread::Builder::new()
            .name("fsync-callbacks".into())
            .spawn(move || {
                while let Ok(o) = rx.recv() {
                    // A panicking app callback must not take the dispatcher down.
                    let _ = catch_unwind(AssertUnwindSafe(|| match o {
                        Out::Clip(v) => d.on_remote_clip(v),
                        Out::Device(v) => d.on_device_status(v),
                        Out::Transfer(v) => d.on_transfer(v),
                        Out::Files(v) => d.on_incoming_files(v),
                        Out::Pairing(v) => d.on_pairing_event(v),
                        Out::Account(v) => d.on_account_event(v),
                        Out::Session(v) => d.on_session_changed(v),
                        Out::Error(v) => d.on_error(v),
                    }));
                }
            })
            .ok();
        Self {
            tx: Some(tx),
            thread,
        }
    }

    pub(super) fn send(&self, o: Out) {
        if self.tx.as_ref().is_none_or(|t| t.try_send(o).is_err()) {
            tracing::warn!("callback queue full or closed; event dropped");
        }
    }
}

impl Drop for Dispatcher {
    /// Delivers what is queued, then stops. Does not join when dropped on the callback thread.
    fn drop(&mut self) {
        drop(self.tx.take());
        if let Some(t) = self.thread.take()
            && t.thread().id() != std::thread::current().id()
        {
            let _ = t.join();
        }
    }
}
