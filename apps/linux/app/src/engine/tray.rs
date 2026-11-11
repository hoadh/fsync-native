//! Tray icon (StatusNotifierItem via ksni) on its own thread. Left click toggles the window;
//! the menu offers Open, History, Send clipboard now, Paste received clip (only while one is held),
//! Pause sync (with a "Sync is paused" line while paused) and Quit. The watcher may appear after
//! us (Waybar starting late): ksni re-registers on its own and tells us through `watcher_*`.
//!
//! ksni re-reads `menu()` only after a click on one of its rows or on `Handle::update`, so every
//! clipboard status change calls `refresh`, which wakes the tray thread to run `update` (never from
//! a menu callback, which runs on the service thread and would wait for itself).
//!
//! State is also shown by which rows exist, not only by row properties: ksni sends `LayoutUpdated`
//! only when the row count changes, and the Omarchy tray (Quickshell) showed a stale check mark
//! after `ItemsPropertiesUpdated` in the 2026-11-12 LIVE run. Every state has its own row count.

use super::clipboard_actions::{Action, request};
use super::clipboard_runtime;
use super::shutdown;
use crate::bridge::clipboard_object::request_error_text;
use crate::core_state::Core;
use ksni::blocking::TrayMethods;
use ksni::menu::{CheckmarkItem, StandardItem};
use ksni::{Icon, MenuItem, OfflineReason};
use std::sync::Arc;
use std::sync::atomic::Ordering;

const TRAY_PNG: &[u8] = include_bytes!("../../../resources/icons/tray/fsync-tray-32.png");

struct FsyncTray {
    core: Arc<Core>,
}

/// PNG -> ARGB32 (network byte order) as the SNI `IconPixmap` wants it.
fn tray_pixmap() -> Vec<Icon> {
    let Ok(img) = image::load_from_memory_with_format(TRAY_PNG, image::ImageFormat::Png) else {
        return Vec::new();
    };
    let rgba = img.to_rgba8();
    // rgba -> argb
    let data = rgba
        .pixels()
        .flat_map(|p| [p[3], p[0], p[1], p[2]])
        .collect();
    vec![Icon {
        width: img.width() as i32,
        height: img.height() as i32,
        data,
    }]
}

impl FsyncTray {
    /// Tray send / paste: the tray's own activation token lets the GNOME window take the focus.
    fn clip_action(&self, action: Action) {
        let token = ksni::current_xdg_activation_token();
        if let Err(code) = request(&self.core, action, token) {
            self.core
                .update_ui(|u| u.message = request_error_text(code).into());
        }
    }

    fn set_registered(&self, on: bool) {
        self.core.tray_registered.store(on, Ordering::SeqCst);
        self.core.hub.poke_engine();
    }
}

impl ksni::Tray for FsyncTray {
    fn id(&self) -> String {
        fsync_linux_platform::APP_ID.into()
    }

    fn title(&self) -> String {
        "fsync".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        tray_pixmap()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.core.hub.toggle_window();
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let status = clipboard_runtime::snapshot(&self.core);
        let pending = (!status.pending.is_empty()).then_some(status.pending);
        let mut items: Vec<MenuItem<Self>> = vec![
            StandardItem {
                label: "Open fsync".into(),
                activate: Box::new(|t: &mut Self| {
                    t.core.hub.show_window(ksni::current_xdg_activation_token());
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "History".into(),
                activate: Box::new(|t: &mut Self| {
                    t.core
                        .hub
                        .show_section(ksni::current_xdg_activation_token(), "history");
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Send clipboard now".into(),
                activate: Box::new(|t: &mut Self| t.clip_action(Action::Send)),
                ..Default::default()
            }
            .into(),
        ];
        items.extend(state_rows(status.paused, pending));
        items.extend([
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|t: &mut Self| shutdown::begin(Some(Arc::clone(&t.core)))),
                ..Default::default()
            }
            .into(),
        ]);
        items
    }

    fn watcher_online(&self) {
        tracing::info!("tray: StatusNotifierWatcher is online");
        self.set_registered(true);
    }

    fn watcher_offline(&self, reason: OfflineReason) -> bool {
        tracing::info!("tray: no StatusNotifierWatcher ({reason:?}); waiting for one");
        self.set_registered(false);
        true // keep running: ksni re-registers when a watcher appears
    }
}

/// The clipboard status changed (pause, a held clip, ...): the tray menu must be re-read.
pub fn refresh(core: &Core) {
    core.tray_dirty.store(true, Ordering::SeqCst);
    if let Some(t) = core.tray_thread.get() {
        t.unpark();
    }
}

/// The rows that depend on the clipboard state. Each state change adds or removes a row, so every
/// tray host refreshes the menu (see the module doc).
fn state_rows(paused: bool, pending: Option<String>) -> Vec<MenuItem<FsyncTray>> {
    let mut rows = Vec::new();
    if let Some(p) = pending {
        rows.push(
            StandardItem {
                label: format!("Paste received clip ({p})"),
                activate: Box::new(|t: &mut FsyncTray| t.clip_action(Action::Paste)),
                ..Default::default()
            }
            .into(),
        );
        // Two rows for a held clip, one for "paused": all four states differ in row count.
        rows.push(MenuItem::Separator);
    }
    if paused {
        rows.push(
            StandardItem {
                label: "Sync is paused".into(),
                enabled: false,
                ..Default::default()
            }
            .into(),
        );
    }
    rows.push(
        CheckmarkItem {
            label: "Pause sync".into(),
            checked: paused,
            activate: Box::new(|t: &mut FsyncTray| clipboard_runtime::toggle_pause(&t.core)),
            ..Default::default()
        }
        .into(),
    );
    rows
}

/// Starts the tray thread. Without a session bus or watcher the app simply has no tray.
pub fn start(core: Arc<Core>) {
    let spawned = std::thread::Builder::new()
        .name("fsync-tray".into())
        .spawn(move || {
            // Optimistic before spawn: a missing watcher calls `watcher_offline` during spawn.
            core.tray_registered.store(true, Ordering::SeqCst);
            let tray = FsyncTray {
                core: Arc::clone(&core),
            };
            match tray.assume_sni_available(true).spawn() {
                Ok(handle) => {
                    let _ = core.tray_thread.set(std::thread::current());
                    core.hub.poke_engine();
                    // Holding the handle keeps the service alive; a wake-up with the dirty flag set
                    // re-reads the menu (several status changes collapse into one update).
                    // The flag is checked before parking: a change made before `tray_thread` was set
                    // (it could not wake us) is picked up on the first pass.
                    loop {
                        if core.tray_dirty.swap(false, Ordering::SeqCst) {
                            handle.update(|_| ());
                        }
                        std::thread::park();
                    }
                }
                Err(e) => {
                    tracing::warn!("tray unavailable: {e}");
                    core.tray_registered.store(false, Ordering::SeqCst);
                    core.hub.poke_engine();
                }
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("cannot start tray thread: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(rows: &[MenuItem<FsyncTray>]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                MenuItem::Standard(i) => i.label.clone(),
                MenuItem::Checkmark(i) => format!("{} [{}]", i.label, i.checked),
                _ => "other".into(),
            })
            .collect()
    }

    #[test]
    fn every_state_has_its_own_row_count() {
        let clip = || Some("text from rock".to_owned());
        let idle = labels(&state_rows(false, None));
        let paused = labels(&state_rows(true, None));
        let pending = labels(&state_rows(false, clip()));
        let both = labels(&state_rows(true, clip()));
        assert_eq!(idle, ["Pause sync [false]"]);
        assert_eq!(paused, ["Sync is paused", "Pause sync [true]"]);
        assert_eq!(
            pending,
            [
                "Paste received clip (text from rock)",
                "other",
                "Pause sync [false]"
            ]
        );
        assert_eq!(
            both,
            [
                "Paste received clip (text from rock)",
                "other",
                "Sync is paused",
                "Pause sync [true]"
            ]
        );
        // Any change between two states changes the row count, so ksni sends LayoutUpdated.
        let mut counts = [idle.len(), paused.len(), pending.len(), both.len()];
        counts.sort_unstable();
        assert_eq!(counts, [1, 2, 3, 4]);
    }
}
