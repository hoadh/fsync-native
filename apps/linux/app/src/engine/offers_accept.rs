//! The accept flow of a pushed-files offer: pulls every file into the download folder one by one
//! and records the outcome. Runs on an op thread; cancel flags come from the Cancel button,
//! sign-out and shutdown.

use super::clip_service_adapter::ClipServiceAdapter;
use super::offers::files_text;
use crate::core_state::Core;
use crate::core_state::files_state::{OfferState, lock};
use fsync_linux_platform::files::download_dir::push_dir;
use fsync_linux_platform::paths::{create_private_dir, process_env};
use fsync_linux_platform::sync::ClipService;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// A push of many gigabytes may take hours on a slow link; this only ends a stuck transfer.
const PUSH_BUDGET: Duration = Duration::from_secs(24 * 3600);

pub(super) fn accept_blocking(core: &Arc<Core>, item_id: &str) {
    let (files, cancel) = {
        let mut offers = lock(&core.files.offers);
        let Some(o) = offers
            .iter_mut()
            .find(|o| o.item_id == item_id && o.state == OfferState::Pending)
        else {
            return;
        };
        o.state = OfferState::Receiving;
        (o.files.clone(), o.cancel.clone())
    };
    core.hub.poke_offers();
    let cancelled = || cancel.load(Ordering::SeqCst);
    let outcome = (|| -> Result<PathBuf, String> {
        let dest = push_dir(&process_env).ok_or("There is no download folder.")?;
        create_private_dir(&dest).map_err(|e| {
            tracing::warn!("cannot create the download folder: {e}");
            "The download folder cannot be created.".to_owned()
        })?;
        let svc = ClipServiceAdapter {
            core: Arc::clone(core),
        };
        let deadline = Instant::now() + PUSH_BUDGET;
        for f in &files {
            if cancelled() {
                return Err("Cancelled.".into());
            }
            svc.fetch_file(item_id, f.file_id, &dest, &cancelled, deadline)?;
            if let Some(o) = lock(&core.files.offers)
                .iter_mut()
                .find(|o| o.item_id == item_id)
            {
                o.finished_bytes = o.finished_bytes.saturating_add(f.size);
                o.active_bytes = 0;
            }
        }
        Ok(dest)
    })();
    let (state, dir, from) = {
        let mut offers = lock(&core.files.offers);
        let Some(o) = offers.iter_mut().find(|o| o.item_id == item_id) else {
            return;
        };
        o.active_bytes = 0;
        o.active_transfer = None;
        match &outcome {
            Ok(dest) => {
                o.state = OfferState::Done;
                o.dir = dest.to_string_lossy().into_owned();
                o.finished_bytes = o.total;
            }
            Err(_) if cancelled() => o.state = OfferState::Cancelled,
            Err(e) => {
                o.state = OfferState::Failed;
                o.message.clone_from(e);
            }
        }
        (o.state, o.dir.clone(), o.from_name.clone())
    };
    core.hub.poke_offers();
    if state == OfferState::Done {
        fsync_linux_platform::notify::show(
            "Files received",
            &format!("{} from {from} saved to {dir}", files_text(files.len())),
        );
    }
}
