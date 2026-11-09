//! The inbox of received file clips: where it lives, what happens to a clip too large to fetch
//! without asking, the explicit download, and retention (at start, every 24 h, on a settings
//! change; ADR 013 d13).

use super::clip_service_adapter::ClipServiceAdapter;
use super::ops::spawn;
use super::{clipboard_runtime, xdg_open};
use crate::core_state::Core;
use crate::core_state::gate::OpKind;
use fsync_core::service::ClipItem;
use fsync_linux_platform::files::inbox::{DownloadGuard, FetchError, Inbox};
use fsync_linux_platform::files::retention::{Limits, sweep};
use fsync_linux_platform::sync::ServiceFetcher;
use std::sync::{Arc, Weak};
use std::time::{Duration, SystemTime};

/// An explicit download of a large clip may take a while (the eager one is bounded by 180 s).
const DOWNLOAD_BUDGET: Duration = Duration::from_secs(30 * 60);
const RETENTION_EVERY: Duration = Duration::from_secs(24 * 3600);
const WAKE: Duration = Duration::from_secs(30);

pub fn inbox(core: &Core) -> Inbox {
    core.inbox.clone()
}

/// A file clip above the eager limit arrived: keep it for the History Download button and tell
/// the user (no notification action: ADR 014). Runs on the inbound lane, so it must not block.
pub fn on_large(core: &Arc<Core>, item: &ClipItem) {
    core.files.hold_large(item.clone());
    let mib = item.total_size.div_ceil(1 << 20);
    let spawned = std::thread::Builder::new()
        .name("fsync-notify".into())
        .spawn(move || {
            fsync_linux_platform::notify::show(
                "Large file clip",
                &format!("Large file clip ({mib} MiB) - open fsync to download"),
            );
        });
    if let Err(e) = spawned {
        tracing::warn!("cannot start the notification thread: {e}");
    }
}

/// Downloads a held large clip into the inbox; with `open` the folder is shown afterwards.
/// The caller holds the claim (`Inbox::begin_download`), so a duplicate was already refused.
pub fn download_blocking(
    core: &Arc<Core>,
    guard: &DownloadGuard,
    item_id: &str,
    open: bool,
) -> Result<(), String> {
    let item = core
        .files
        .large_item(item_id)
        .ok_or("This download is no longer available. Copy the files again on the other device.")?;
    let adapter = ClipServiceAdapter {
        core: Arc::clone(core),
    };
    let inbox = inbox(core);
    let epoch = core.files.epoch();
    // Sign-out and shutdown bump the epoch: the download stops and its partial copy is removed.
    let cancelled = || core.files.epoch() != epoch;
    match inbox.fetch_item_guarded(
        guard,
        &ServiceFetcher(&adapter),
        &item,
        &cancelled,
        DOWNLOAD_BUDGET,
    ) {
        Ok(_) => {
            // The user asked for it: retention keeps it until it ages out (ADR 014).
            inbox.mark_explicit(item_id);
            if open {
                match inbox.item_dir(item_id) {
                    Ok(dir) => {
                        xdg_open::open_dir(&dir);
                    }
                    Err(_) => return Err("The folder cannot be opened.".into()),
                }
            }
            Ok(())
        }
        Err(FetchError::Cancelled) => Err("Cancelled.".into()),
        Err(FetchError::Failed(e)) => Err(e),
    }
}

/// Sign-out of any kind (logout, session expired, device removed): running downloads and
/// accepted offers stop, the account's offers, held clips and History rows are forgotten, and a
/// History load already in flight is made stale so it cannot bring the rows back.
pub fn reset_account_files(core: &Core) {
    // Invalidate first: a History load that passes its under-lock check before this point
    // writes its rows before `reset` takes the same lock and clears them.
    core.gate.invalidate(OpKind::History);
    core.files.reset();
    // Files staged for the DevicePicker belong to the signed-out account: forget them (this
    // also closes the picker).
    super::send_files::reset_for_sign_out(core);
    core.hub.poke_offers();
    core.hub.poke_history();
}

/// One retention pass with core's current settings.
pub fn run_retention(core: &Core) {
    let Some(settings) = core.service().and_then(|s| s.settings().ok()) else {
        return;
    };
    let limits = Limits {
        max_age: Duration::from_secs(u64::from(settings.retention_days) * 86_400),
        max_bytes: settings.retention_bytes,
    };
    let current = clipboard_runtime::engine(core).and_then(|e| e.current_file_item());
    let keep: Vec<&str> = current.as_deref().into_iter().collect();
    // In-flight downloads are protected by their claim, taken per item right before removal.
    let claim = |id: &str| core.inbox.begin_download(id).ok();
    match sweep(inbox(core).root(), limits, &keep, &claim, SystemTime::now()) {
        Ok(r) if r.removed > 0 => {
            tracing::info!(
                "inbox retention removed {} item(s), {} bytes",
                r.removed,
                r.freed
            );
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("inbox retention failed: {e}"),
    }
}

/// Retention after a settings change (history retention days or size); called by the
/// Settings screen (`settings_ops::save`).
pub fn retention_soon(core: &Arc<Core>) {
    spawn(core, "inbox-retention", |c| run_retention(c));
}

/// At start and then every 24 h; stops when the app shuts down.
pub fn start_schedule(core: &Arc<Core>) {
    let weak: Weak<Core> = Arc::downgrade(core);
    let spawned = std::thread::Builder::new()
        .name("fsync-inbox-retention".into())
        .spawn(move || {
            loop {
                let Some(core) = weak.upgrade() else { return };
                if !core.gate.is_open() {
                    return;
                }
                run_retention(&core);
                drop(core);
                let mut waited = Duration::ZERO;
                while waited < RETENTION_EVERY {
                    std::thread::sleep(WAKE);
                    waited += WAKE;
                    match weak.upgrade() {
                        Some(c) if c.gate.is_open() => {}
                        _ => return,
                    }
                }
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("cannot start the inbox retention thread: {e}");
    }
}
