//! `ClipService` over the running `FsyncService`, plus the "signed in and online" switch: the
//! clipboard is not read while the account is not ready.

use super::user_messages;
use crate::core_state::{Core, lock};
use fsync_core::service::{ClipMode, LocalClip, Published};
use fsync_linux_platform::files::transfer_wait::Outcome;
use fsync_linux_platform::sync::{ClipService, SyncSettings};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

pub struct ClipServiceAdapter {
    pub core: Arc<Core>,
}

const NOT_READY: &str = "The engine is not ready yet.";

impl ClipService for ClipServiceAdapter {
    fn publish(&self, clip: LocalClip) -> Result<Published, String> {
        let svc = self.core.service().ok_or(NOT_READY)?;
        svc.publish_clip(clip)
            .map_err(|e| user_messages::for_error(&e))
    }

    fn fetch_rep(&self, item_id: &str, idx: u32) -> Result<Vec<u8>, String> {
        let svc = self.core.service().ok_or(NOT_READY)?;
        svc.fetch_rep(item_id, idx)
            .map_err(|e| user_messages::for_error(&e))
    }

    /// Starts core's file pull and waits for its end (the delegate feeds `core.transfers`).
    fn fetch_file(
        &self,
        item_id: &str,
        file_id: u32,
        dest_dir: &Path,
        cancel: &dyn Fn() -> bool,
        deadline: Instant,
    ) -> Result<PathBuf, String> {
        let svc = self.core.service().ok_or(NOT_READY)?;
        let dest = dest_dir
            .to_str()
            .ok_or("The destination folder name is not valid text.")?;
        let id = svc
            .fetch_file_to(item_id, file_id, dest)
            .map_err(|e| user_messages::for_error(&e))?;
        match self.core.transfers.wait(id, cancel, deadline) {
            Outcome::Done(path) => Ok(PathBuf::from(path)),
            // Core's text can name a file or a path: the user gets a fixed sentence.
            Outcome::Failed(_) => Err("The transfer failed.".into()),
            Outcome::Cancelled => Err("Cancelled.".into()),
            Outcome::Aborted | Outcome::TimedOut => {
                // Stop the pull in core too, else it keeps downloading for nobody.
                if let Err(e) = svc.cancel(id) {
                    tracing::debug!("cancel after abort: {e:?}");
                }
                Err(if cancel() {
                    "Cancelled.".into()
                } else {
                    "It took too long to arrive.".into()
                })
            }
        }
    }

    fn settings(&self) -> SyncSettings {
        let active = lock(&self.core.ui).state.is_ready();
        let from_core = self.core.service().and_then(|s| match s.settings() {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!("cannot read clipboard settings, using defaults: {e}");
                None
            }
        });
        match from_core {
            Some(s) => SyncSettings {
                active,
                auto: s.clip_mode == ClipMode::Auto,
                max_clip_bytes: s.max_clip_bytes,
                denylist: s.denylist,
            },
            None => SyncSettings {
                active,
                ..SyncSettings::default()
            },
        }
    }
}
