//! Handles to the live Qt objects. Worker threads never touch Qt objects directly: they poke
//! through these `CxxQtThread`s, whose closures run on the GUI thread.

use crate::bridge::clipboard_object::qobject::ClipboardObject;
use crate::bridge::devices_model::qobject::DevicesModel;
use crate::bridge::engine_object::qobject::EngineObject;
use crate::bridge::history_model::qobject::HistoryModel;
use crate::bridge::pairing_object::qobject::PairingObject;
use crate::bridge::send_object::qobject::SendObject;
use crate::bridge::transfers_object::qobject::TransfersObject;
use cxx_qt::CxxQtThread;
use cxx_qt_lib::QString;
use fsync_linux_platform::clipboard::OwnedEntry;
use std::sync::Mutex;

#[derive(Default)]
pub struct Hub {
    engine: Mutex<Option<CxxQtThread<EngineObject>>>,
    pairing: Mutex<Option<CxxQtThread<PairingObject>>>,
    devices: Mutex<Option<CxxQtThread<DevicesModel>>>,
    clipboard: Mutex<Option<CxxQtThread<ClipboardObject>>>,
    history: Mutex<Option<CxxQtThread<HistoryModel>>>,
    offers: Mutex<Option<CxxQtThread<TransfersObject>>>,
    send: Mutex<Option<CxxQtThread<SendObject>>>,
}

fn guard<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Hub {
    pub fn register_engine(&self, t: CxxQtThread<EngineObject>) {
        *guard(&self.engine) = Some(t);
    }

    pub fn register_pairing(&self, t: CxxQtThread<PairingObject>) {
        *guard(&self.pairing) = Some(t);
    }

    pub fn register_devices(&self, t: CxxQtThread<DevicesModel>) {
        *guard(&self.devices) = Some(t);
    }

    pub fn register_clipboard(&self, t: CxxQtThread<ClipboardObject>) {
        *guard(&self.clipboard) = Some(t);
    }

    pub fn register_history(&self, t: CxxQtThread<HistoryModel>) {
        *guard(&self.history) = Some(t);
    }

    pub fn register_offers(&self, t: CxxQtThread<TransfersObject>) {
        *guard(&self.offers) = Some(t);
    }

    pub fn register_send(&self, t: CxxQtThread<SendObject>) {
        *guard(&self.send) = Some(t);
    }

    pub fn has_engine(&self) -> bool {
        guard(&self.engine).is_some()
    }

    // A failed queue means the object is gone (app quitting): nothing left to update.
    pub fn poke_engine(&self) {
        if let Some(t) = guard(&self.engine).as_ref() {
            let _ = t.queue(|o| o.refresh());
        }
    }

    pub fn poke_pairing(&self) {
        if let Some(t) = guard(&self.pairing).as_ref() {
            let _ = t.queue(|o| o.refresh());
        }
    }

    pub fn poke_devices(&self) {
        if let Some(t) = guard(&self.devices).as_ref() {
            let _ = t.queue(|o| o.reload());
        }
    }

    pub fn poke_history(&self) {
        if let Some(t) = guard(&self.history).as_ref() {
            let _ = t.queue(|o| o.reload());
        }
    }

    /// The staged send set changed (picker summary / close).
    pub fn poke_send(&self) {
        if let Some(t) = guard(&self.send).as_ref() {
            let _ = t.queue(|o| o.refresh());
        }
    }

    pub fn poke_offers(&self) {
        if let Some(t) = guard(&self.offers).as_ref() {
            let _ = t.queue(|o| o.reload());
        }
    }

    /// History copy-back without a background backend: the focused window writes `entry`
    /// through Qt on the GUI thread. False when no History model exists (nothing will write).
    pub fn write_from_window(&self, entry: OwnedEntry) -> bool {
        let Some(t) = guard(&self.history).as_ref().cloned() else {
            return false;
        };
        t.queue(move |o| o.write_entry(&entry)).is_ok()
    }

    /// Raises the window on one section ("history", "offers").
    pub fn show_section(&self, token: Option<String>, section: &'static str) {
        if let Some(t) = guard(&self.engine).as_ref() {
            let token = token.unwrap_or_default();
            let _ = t.queue(move |mut o| {
                o.as_mut().emit_show_window(QString::from(&token));
                o.emit_section(QString::from(section));
            });
        }
    }

    pub fn poke_clipboard(&self) {
        if let Some(t) = guard(&self.clipboard).as_ref() {
            let _ = t.queue(|o| o.refresh());
        }
    }

    /// Opens the QuickClipWindow (`action`: "send" or "paste"); `token` is the launcher's
    /// xdg-activation token, which the window needs to get the keyboard focus.
    /// False when no window object is registered (nothing will open).
    pub fn quick_clip(&self, action: &'static str, token: Option<String>) -> bool {
        let Some(t) = guard(&self.clipboard).as_ref().cloned() else {
            return false;
        };
        let token = token.unwrap_or_default();
        t.queue(move |o| {
            o.emit_quick_clip(QString::from(action), QString::from(&token));
        })
        .is_ok()
    }

    /// Asks QML to raise the main window; `token` is the launcher's xdg-activation token.
    pub fn show_window(&self, token: Option<String>) {
        if let Some(t) = guard(&self.engine).as_ref() {
            let token = token.unwrap_or_default();
            let _ = t.queue(move |o| o.emit_show_window(QString::from(&token)));
        }
    }

    /// Tray left click: show the window, or hide it when it is already in front.
    pub fn toggle_window(&self) {
        if let Some(t) = guard(&self.engine).as_ref() {
            let _ = t.queue(|o| o.emit_toggle_window());
        }
    }

    /// Final shutdown step: QML calls `Qt.quit()` in reply.
    pub fn quit(&self) {
        if let Some(t) = guard(&self.engine).as_ref() {
            let _ = t.queue(|o| o.emit_quit());
        }
    }
}
