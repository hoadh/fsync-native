//! Tray icon (StatusNotifierItem via ksni) on its own thread. Left click toggles the window;
//! the menu offers Open, History, Send clipboard now, Paste received clip (enabled while one is held),
//! Pause sync and Quit. The watcher may appear after
//! us (Waybar starting late): ksni re-registers on its own and tells us through `watcher_*`.

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
        let paused = status.paused;
        let pending = (!status.pending.is_empty()).then_some(status.pending);
        vec![
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
            StandardItem {
                label: match pending.as_deref() {
                    Some(p) => format!("Paste received clip ({p})"),
                    None => "Paste received clip".into(),
                },
                enabled: pending.is_some(),
                activate: Box::new(|t: &mut Self| t.clip_action(Action::Paste)),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Pause sync".into(),
                checked: paused,
                activate: Box::new(|t: &mut Self| clipboard_runtime::toggle_pause(&t.core)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|t: &mut Self| shutdown::begin(Some(Arc::clone(&t.core)))),
                ..Default::default()
            }
            .into(),
        ]
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
                    core.hub.poke_engine();
                    // Park: dropping the handle would stop the tray service.
                    loop {
                        std::thread::park();
                        let _keep_alive = &handle;
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
