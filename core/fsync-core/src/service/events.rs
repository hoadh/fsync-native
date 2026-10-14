//! Engine events -> app callbacks.

use super::{
    convert::{clip_item, reset_id},
    delegate::Out,
    error::FsyncError,
    inner::Inner,
    types::AccountEvent,
};
use crate::events::EngineEvent;
use std::sync::Arc;
use tokio::sync::mpsc;

pub(super) async fn pump(inner: Arc<Inner>, mut rx: mpsc::Receiver<EngineEvent>) {
    while let Some(ev) = rx.recv().await {
        inner.on_event(ev);
    }
}

impl Inner {
    fn on_event(self: &Arc<Self>, ev: EngineEvent) {
        match ev {
            EngineEvent::RemoteClip { from, offer } => {
                self.emit(Out::Clip(clip_item(&self.trust, &from, &offer)));
            }
            EngineEvent::IncomingFiles { from, offer } => {
                self.emit(Out::Files(clip_item(&self.trust, &from, &offer)));
            }
            EngineEvent::FileServed {
                item_id, file_id, ..
            } => {
                let evs = match super::inner::lock(&self.xfers) {
                    Ok(mut x) => x.on_served(item_id, file_id),
                    Err(_) => return,
                };
                self.emit_all(evs);
            }
            EngineEvent::DeviceStatus { node_id, .. } => self.emit_device(&node_id),
            EngineEvent::ListAdvanced { seq } => {
                self.emit(Out::Account(AccountEvent::ListAdvanced { seq }));
            }
            EngineEvent::Pairing(p) => self.on_pairing_event(p),
            EngineEvent::SelfRemoved => {
                if let Ok(mut l) = super::inner::lock(&self.live) {
                    l.presence = None;
                }
                self.emit(Out::Account(AccountEvent::SelfRemoved));
            }
            EngineEvent::TrustReset {
                head_hash,
                device_names,
            } => {
                self.emit(Out::Account(AccountEvent::TrustReset {
                    reset_id: reset_id(&head_hash),
                    devices: u32::try_from(device_names.len()).unwrap_or(u32::MAX),
                    device_names,
                }));
            }
            EngineEvent::Error { message } => {
                self.emit(Out::Error(FsyncError::Internal(message)));
            }
            EngineEvent::HistoryReset { backup } => {
                self.emit(Out::Account(AccountEvent::HistoryReset {
                    backup: backup.to_string_lossy().into_owned(),
                }));
            }
        }
    }
}
