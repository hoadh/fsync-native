//! `ServiceDelegate` for the app: runs on core's single "fsync-callbacks" thread, so nothing here
//! may block for long. Token saves wait at most 3 s on the secrets thread (never oo7 here);
//! everything else only updates snapshots or starts an op thread.

use super::secrets_sync::{CALLBACK_SAVE_WAIT, from_core_tokens, persist};
use super::{
    clipboard_runtime, offers, ops_account, ops_devices, ops_pairing_events, user_messages,
};
use crate::core_state::Core;
use fsync_core::service::{
    AccountEvent, ClipItem, Device, FsyncError, PairingEvent, ServiceDelegate, SessionTokens,
    TransferEvent,
};
use std::sync::Arc;

pub struct DelegateAdapter {
    core: Arc<Core>,
}

impl DelegateAdapter {
    pub fn new(core: Arc<Core>) -> Self {
        Self { core }
    }
}

impl ServiceDelegate for DelegateAdapter {
    fn on_remote_clip(&self, item: ClipItem) {
        // Never blocks: the clip is queued on the inbound lane (eager fetch happens there).
        if let Some(engine) = clipboard_runtime::engine(&self.core) {
            engine.on_remote_clip(item);
        }
    }

    fn on_transfer(&self, event: TransferEvent) {
        offers::on_transfer(&self.core, event);
    }

    fn on_incoming_files(&self, offer: ClipItem) {
        offers::on_incoming(&self.core, offer);
    }

    fn on_device_status(&self, device: Device) {
        ops_devices::on_status(&self.core, &device);
    }

    fn on_pairing_event(&self, event: PairingEvent) {
        ops_pairing_events::on_event(&self.core, event);
    }

    fn on_account_event(&self, event: AccountEvent) {
        ops_account::on_event(&self.core, event);
    }

    fn on_session_changed(&self, tokens: SessionTokens) {
        let saved = from_core_tokens(&tokens);
        // On timeout the tokens stay in memory (mirror) and the banner says so; the next change
        // or the quit flush retries.
        persist(&self.core, CALLBACK_SAVE_WAIT, |s| s.session = Some(saved));
    }

    fn on_error(&self, error: FsyncError) {
        let text = user_messages::for_error(&error);
        self.core.update_ui(|u| u.message = text);
    }
}
