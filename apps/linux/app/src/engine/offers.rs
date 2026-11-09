//! Pushed files ("Send to this device"): offers, accept / decline / cancel, progress, and the
//! auto-accept rule (setting on and total <= 1 GiB, ADR 013 d14). Accepted files are saved into
//! `<download dir>/fsync` (0700). All network work runs on op threads; callbacks only queue.

use super::offers_accept::accept_blocking;
use super::ops::spawn;
use super::xdg_open;
use crate::core_state::Core;
use crate::core_state::files_state::{Admit, OfferRow, OfferState, admit_offer, lock};
use fsync_core::service::{ClipItem, TransferDirection, TransferEvent, TransferState};
use fsync_linux_platform::files::should_auto_accept;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

const PROGRESS_EVERY: Duration = Duration::from_millis(250);

pub(super) fn files_text(n: usize) -> String {
    if n == 1 {
        "1 file".into()
    } else {
        format!("{n} files")
    }
}

/// Core callback: books the offer at once (short lock, bounded list) and only then starts an op
/// thread for the notification and the auto-accept; a burst of pushes cannot pile up threads.
pub fn on_incoming(core: &Arc<Core>, offer: ClipItem) {
    let row = OfferRow::new(&offer);
    let admitted = admit_offer(&mut lock(&core.files.offers), row.clone());
    core.hub.poke_offers();
    if admitted != Admit::Added {
        return;
    }
    spawn(core, "offer-in", move |c| {
        let auto = should_auto_accept(
            c.service()
                .and_then(|s| s.settings().ok())
                .is_some_and(|s| s.auto_accept_files),
            offer.total_size,
        );
        let body = format!("{} from {}", files_text(offer.files.len()), row.from_name);
        if auto {
            fsync_linux_platform::notify::show("Receiving files", &body);
            accept_blocking(c, &offer.item_id);
        } else if c.files.notify_due() {
            fsync_linux_platform::notify::show("Files offered: open fsync to accept", &body);
        }
    });
}

/// Core callback: feeds the waiters and the progress of the offer being received.
pub fn on_transfer(core: &Arc<Core>, ev: TransferEvent) {
    core.transfers.on_event(&ev);
    super::send_files::on_send_event(core, &ev);
    if ev.direction != TransferDirection::Receive
        || !matches!(ev.state, TransferState::Started | TransferState::Progress)
    {
        return;
    }
    let changed = {
        let mut offers = lock(&core.files.offers);
        match offers
            .iter_mut()
            .find(|o| o.item_id == ev.item_id && o.state == OfferState::Receiving)
        {
            Some(o) => {
                o.active_bytes = ev.bytes_done;
                o.active_transfer = Some(ev.transfer_id);
                true
            }
            None => false,
        }
    };
    if !changed {
        return;
    }
    // At most 4 UI updates per second; the final state change always pokes.
    let mut last = lock(&core.files.last_progress_poke);
    if last.is_none_or(|t| t.elapsed() >= PROGRESS_EVERY) {
        *last = Some(Instant::now());
        drop(last);
        core.hub.poke_offers();
    }
}

pub fn accept(core: &Arc<Core>, item_id: String) {
    spawn(core, "offer-accept", move |c| accept_blocking(c, &item_id));
}

/// Accepts the newest pending offer, or `item_id` (e2e). `Err` says why nothing started.
#[cfg(feature = "e2e-control")]
pub fn accept_pending(core: &Arc<Core>, item_id: Option<String>) -> Result<String, String> {
    let id = {
        let offers = lock(&core.files.offers);
        offers
            .iter()
            .rev()
            .find(|o| {
                o.state == OfferState::Pending && item_id.as_ref().is_none_or(|i| *i == o.item_id)
            })
            .map(|o| o.item_id.clone())
    }
    .ok_or("no pending offer")?;
    accept(core, id.clone());
    Ok(id)
}

pub fn decline(core: &Arc<Core>, item_id: &str) {
    set_state(core, item_id, OfferState::Pending, OfferState::Declined, "");
}

pub fn cancel(core: &Arc<Core>, item_id: &str) {
    let flag = lock(&core.files.offers)
        .iter()
        .find(|o| o.item_id == item_id && o.state == OfferState::Receiving)
        .map(|o| o.cancel.clone());
    if let Some(f) = flag {
        f.store(true, Ordering::SeqCst);
    }
}

/// Removes a finished offer from the list.
pub fn dismiss(core: &Arc<Core>, item_id: &str) {
    lock(&core.files.offers).retain(|o| !(o.item_id == item_id && o.state.finished()));
    core.hub.poke_offers();
}

pub fn open_folder(core: &Core, item_id: &str) {
    let dir = lock(&core.files.offers)
        .iter()
        .find(|o| o.item_id == item_id && o.state == OfferState::Done)
        .map(|o| PathBuf::from(&o.dir));
    if let Some(d) = dir {
        xdg_open::open_dir(&d);
    }
}

pub(super) fn set_state(
    core: &Core,
    item_id: &str,
    from: OfferState,
    to: OfferState,
    message: &str,
) {
    let mut changed = false;
    if let Some(o) = lock(&core.files.offers)
        .iter_mut()
        .find(|o| o.item_id == item_id && o.state == from)
    {
        o.state = to;
        o.message = message.to_owned();
        changed = true;
    }
    if changed {
        core.hub.poke_offers();
    }
}

/// For the e2e status.
#[cfg(feature = "e2e-control")]
pub fn snapshot_json(core: &Core) -> Vec<serde_json::Value> {
    lock(&core.files.offers)
        .iter()
        .map(|o| {
            serde_json::json!({
                "item_id": o.item_id, "state": o.state.as_str(), "from": o.from_name,
                "count": o.files.len(), "total": o.total, "done": o.done_bytes(),
                "dir": o.dir, "message": o.message,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "offers_tests.rs"]
mod tests;
