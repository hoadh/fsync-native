//! Delegate that turns service callbacks into a channel the commands read.

use crate::session;
use fsync_core::service::*;
use std::{path::PathBuf, sync::mpsc::Sender};

pub enum Ev {
    Clip(ClipItem),
    Files(ClipItem),
    Transfer(TransferEvent),
    Pairing(PairingEvent),
    Account(AccountEvent),
    Error(FsyncError),
}

pub struct CliDelegate {
    pub tx: Sender<Ev>,
    pub session_path: PathBuf,
}

impl CliDelegate {
    fn put(&self, e: Ev) {
        // The receiver is gone when the command finished; nothing to deliver then.
        let _ = self.tx.send(e);
    }
}

impl ServiceDelegate for CliDelegate {
    fn on_remote_clip(&self, item: ClipItem) {
        self.put(Ev::Clip(item));
    }
    fn on_device_status(&self, _device: Device) {}
    fn on_transfer(&self, event: TransferEvent) {
        self.put(Ev::Transfer(event));
    }
    fn on_incoming_files(&self, offer: ClipItem) {
        self.put(Ev::Files(offer));
    }
    fn on_pairing_event(&self, event: PairingEvent) {
        self.put(Ev::Pairing(event));
    }
    fn on_account_event(&self, event: AccountEvent) {
        self.put(Ev::Account(event));
    }
    fn on_session_changed(&self, tokens: SessionTokens) {
        if let Err(e) = session::save(&self.session_path, &tokens) {
            self.put(Ev::Error(FsyncError::Io(e.to_string())));
        }
    }
    fn on_error(&self, error: FsyncError) {
        self.put(Ev::Error(error));
    }
}
