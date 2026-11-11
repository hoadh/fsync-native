//! State shared by the engine, the watcher thread and both lanes, plus the outbound half of the
//! work (collect, publish). The inbound half is in `shared_inbound`.

use super::lanes::Cancel;
use super::outbound::{OutCtx, collect, describe};
use super::status::StatusCell;
use super::{ClipService, SyncEnv};
use crate::clipboard::echo::WriteGuard;
use crate::clipboard::filters::Skip;
use crate::clipboard::{Backend, ClipChange, OwnedEntry};
use fsync_core::service::LocalClip;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// An automatic change with the same content as the previous automatic publish within this
/// window is the same copy seen twice (some apps set the selection twice per copy, a few ms
/// apart): it is dropped. Explicit sends are never dropped.
pub(super) const AUTO_REPEAT_WINDOW: Duration = Duration::from_secs(1);

pub(super) enum OutJob {
    Auto(ClipChange),
    Explicit(ClipChange),
    Clip(LocalClip),
}

pub(super) struct Shared {
    pub(super) service: Arc<dyn ClipService>,
    pub(super) backend: Option<Arc<dyn Backend>>,
    pub(super) env: SyncEnv,
    pub(super) guard: WriteGuard,
    pub(super) status: StatusCell,
    /// The received clip waiting for an explicit paste (bytes only, newest wins).
    pub(super) pending: Mutex<Option<OwnedEntry>>,
    pub(super) last: Mutex<Option<ClipChange>>,
    /// Inbox item whose files are on (or waiting for) the clipboard: retention keeps it.
    pub(super) current_files: Mutex<Option<String>>,
    pub(super) paused: AtomicBool,
    pub(super) closed: AtomicBool,
    /// Content fingerprint and time of the last automatic publish (see `AUTO_REPEAT_WINDOW`).
    pub(super) last_auto: Mutex<Option<(u64, Instant)>>,
    /// `AUTO_REPEAT_WINDOW` in ms; tests widen it so their timing cannot matter.
    pub(super) repeat_window_ms: AtomicU64,
}

/// Content fingerprint of a clip (not a security hash: it only spots a repeat of our own copy).
fn fingerprint(clip: &LocalClip) -> u64 {
    let mut h = DefaultHasher::new();
    for r in &clip.reps {
        r.mime.hash(&mut h);
        r.data.hash(&mut h);
    }
    clip.files.hash(&mut h);
    h.finish()
}

/// Whether an automatic publish with fingerprint `fp` goes out, recording it when it does. A
/// cancelled job neither sends nor records; a repeat within `window` is dropped.
pub(super) fn admit_auto(
    last: &mut Option<(u64, Instant)>,
    fp: u64,
    cancelled: bool,
    now: Instant,
    window: Duration,
) -> bool {
    if cancelled {
        return false;
    }
    if let Some((prev, at)) = *last
        && prev == fp
        && now.saturating_duration_since(at) < window
    {
        tracing::debug!("clipboard: the same copy seen twice; published once");
        return false;
    }
    *last = Some((fp, now));
    true
}

pub(super) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Shared {
    pub(super) fn run_out(&self, job: OutJob, cancel: &Cancel) {
        let (change, explicit) = match job {
            OutJob::Clip(clip) => return self.publish_explicit(clip, cancel),
            OutJob::Auto(c) => (c, false),
            OutJob::Explicit(c) => (c, true),
        };
        match self.collect_local(&change) {
            Ok(clip) if explicit => self.publish_explicit(clip, cancel),
            Ok(clip) => self.publish_auto(clip, cancel),
            Err(skip) => self.skipped(&skip, explicit),
        }
    }

    /// An explicit publish (tray, hotkey, "send to"): never deduplicated. What peers hold changes,
    /// so the next automatic copy is compared against nothing.
    fn publish_explicit(&self, clip: LocalClip, cancel: &Cancel) {
        if cancel.is_cancelled() {
            return;
        }
        self.forget_last_auto();
        self.send(clip);
    }

    /// An automatic publish, dropped when it repeats the last one within `AUTO_REPEAT_WINDOW`.
    /// The cancel check and the record happen under one lock, after the (possibly slow) hash, and
    /// the send that follows has no second cancel check: a job either records and sends, or
    /// neither, so a cancelled job can never hide the change that replaced it.
    fn publish_auto(&self, clip: LocalClip, cancel: &Cancel) {
        let fp = fingerprint(&clip);
        let admitted = {
            let mut last = lock(&self.last_auto);
            let window = Duration::from_millis(self.repeat_window_ms.load(Ordering::Relaxed));
            admit_auto(&mut last, fp, cancel.is_cancelled(), Instant::now(), window)
        };
        if admitted {
            self.send(clip);
        }
    }

    /// The clipboard or the peers' clip changed another way (a received clip was written, an
    /// explicit send): a later copy of the earlier content is new again.
    pub(super) fn forget_last_auto(&self) {
        *lock(&self.last_auto) = None;
    }

    /// Shows whether background sync is on: a backend exists and core's mode is Auto.
    pub(super) fn note_mode(&self, settings: &super::SyncSettings) {
        let auto = self.backend.is_some() && settings.auto;
        if self.status.get().auto != auto {
            self.status.update(|s| s.auto = auto);
        }
    }

    pub(super) fn collect_local(&self, change: &ClipChange) -> Result<LocalClip, Skip> {
        let settings = self.service.settings();
        collect(
            change,
            &OutCtx {
                settings: &settings,
                paths: &self.env.paths,
                guard: &self.guard,
            },
        )
    }

    pub(super) fn skipped(&self, skip: &Skip, explicit: bool) {
        // Auto: only a problem the user can act on is shown; hints, own echoes and the
        // denylist stay silent (and are never logged with names).
        let shown = explicit || matches!(skip, Skip::TooLarge | Skip::BadImage);
        match (shown, skip.user_text()) {
            (true, Some(t)) => self.status.event(t),
            _ => tracing::debug!("clipboard change skipped: {skip:?}"),
        }
    }

    fn send(&self, clip: LocalClip) {
        // A publish can take long (large files): an event written meanwhile (a received clip)
        // is newer than this outcome and must stay visible.
        let seq = self.status.event_seq();
        match self.service.publish(clip) {
            Ok(p) => {
                if let Some(text) = describe(&p) {
                    self.status.event_if_unchanged(seq, text);
                }
            }
            Err(e) => self
                .status
                .event_if_unchanged(seq, format!("Not sent: {e}")),
        }
    }
}
