use crate::{error::FsyncError, types_account::*, types_clip::*, types_device::*};
use fsync_core::service as svc;
use std::sync::Arc;

/// Implemented by the app. Called from one background thread, in order; slow callbacks delay
/// later ones. Callbacks may call engine methods.
#[uniffi::export(with_foreign)]
pub trait EngineDelegate: Send + Sync {
    fn on_remote_clip(&self, item: ClipItem);
    fn on_device_status(&self, device: Device);
    fn on_transfer(&self, event: TransferEvent);
    fn on_incoming_files(&self, offer: ClipItem);
    fn on_pairing_event(&self, event: PairingEvent);
    fn on_account_event(&self, event: AccountEvent);
    /// Tokens were refreshed: persist them.
    fn on_session_changed(&self, tokens: SessionTokens);
    fn on_error(&self, error: FsyncError);
}

pub(crate) struct Bridge(pub Arc<dyn EngineDelegate>);

impl svc::ServiceDelegate for Bridge {
    fn on_remote_clip(&self, item: svc::ClipItem) {
        self.0.on_remote_clip(item.into());
    }
    fn on_device_status(&self, d: svc::Device) {
        self.0.on_device_status(d.into());
    }
    fn on_transfer(&self, e: svc::TransferEvent) {
        self.0.on_transfer(e.into());
    }
    fn on_incoming_files(&self, o: svc::ClipItem) {
        self.0.on_incoming_files(o.into());
    }
    fn on_pairing_event(&self, e: svc::PairingEvent) {
        self.0.on_pairing_event(e.into());
    }
    fn on_account_event(&self, e: svc::AccountEvent) {
        self.0.on_account_event(e.into());
    }
    fn on_session_changed(&self, t: svc::SessionTokens) {
        self.0.on_session_changed(t.into());
    }
    fn on_error(&self, e: svc::FsyncError) {
        self.0.on_error(e.into());
    }
}
