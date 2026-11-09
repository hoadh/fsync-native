//! `ClipboardObject`: QML singleton with the clipboard status (backend, mode, pending clip, last
//! outcome) and the send / paste / pause actions. Logic is in `crate::engine::clipboard_*`.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, backend_name, cxx_name = "backendName")]
        #[qproperty(QString, reason)]
        #[qproperty(QString, capability)]
        #[qproperty(bool, auto_sync, cxx_name = "autoSync")]
        #[qproperty(bool, sync_paused, cxx_name = "syncPaused")]
        #[qproperty(QString, pending_text, cxx_name = "pendingText")]
        #[qproperty(QString, last_event, cxx_name = "lastEvent")]
        #[qproperty(bool, show_notice, cxx_name = "showNotice")]
        type ClipboardObject = super::ClipboardObjectRust;

        /// Open the QuickClipWindow (`action`: "send" or "paste"; `token`: activation token).
        #[qsignal]
        #[cxx_name = "quickClipRequested"]
        fn quick_clip_requested(self: Pin<&mut ClipboardObject>, action: QString, token: QString);

        #[qinvokable]
        #[cxx_name = "sendNow"]
        fn send_now(self: Pin<&mut ClipboardObject>);
        #[qinvokable]
        #[cxx_name = "pasteNow"]
        fn paste_now(self: Pin<&mut ClipboardObject>);
        #[qinvokable]
        #[cxx_name = "togglePause"]
        fn toggle_pause(self: Pin<&mut ClipboardObject>);
        #[qinvokable]
        #[cxx_name = "dismissNotice"]
        fn dismiss_notice(self: Pin<&mut ClipboardObject>);
    }

    impl cxx_qt::Threading for ClipboardObject {}
    impl cxx_qt::Initialize for ClipboardObject {}
}

use super::app_identity;
use crate::core_state;
use crate::engine::clipboard_actions::{Action, request};
use crate::engine::clipboard_runtime;
use core::pin::Pin;
use cxx_qt::Threading;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct ClipboardObjectRust {
    backend_name: QString,
    reason: QString,
    capability: QString,
    auto_sync: bool,
    sync_paused: bool,
    pending_text: QString,
    last_event: QString,
    show_notice: bool,
}

impl cxx_qt::Initialize for qobject::ClipboardObject {
    fn initialize(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            core.hub.register_clipboard(self.qt_thread());
        }
        self.refresh();
    }
}

impl qobject::ClipboardObject {
    /// Copies the engine's status snapshot into the properties (GUI thread only).
    pub fn refresh(mut self: Pin<&mut Self>) {
        let Some(core) = core_state::get() else {
            return;
        };
        let ui = clipboard_runtime::snapshot(&core);
        self.as_mut().set_backend_name(QString::from(&ui.backend));
        self.as_mut().set_reason(QString::from(&ui.reason));
        self.as_mut().set_capability(QString::from(&ui.capability));
        self.as_mut().set_auto_sync(ui.auto);
        self.as_mut().set_sync_paused(ui.paused);
        self.as_mut().set_pending_text(QString::from(&ui.pending));
        self.as_mut().set_last_event(QString::from(&ui.last_event));
        self.as_mut().set_show_notice(ui.notice);
    }

    /// Opens the QuickClipWindow. The QML handler runs synchronously, so the activation token
    /// is in the environment exactly while its `requestActivate` runs (same as the main window).
    pub fn emit_quick_clip(self: Pin<&mut Self>, action: QString, token: QString) {
        let have_token = app_identity::set_activation_token(&String::from(&token));
        self.quick_clip_requested(action, token);
        if have_token {
            app_identity::clear_activation_token();
        }
    }

    pub fn send_now(self: Pin<&mut Self>) {
        self.act(Action::Send);
    }

    pub fn paste_now(self: Pin<&mut Self>) {
        self.act(Action::Paste);
    }

    fn act(self: Pin<&mut Self>, action: Action) {
        let Some(core) = core_state::get() else {
            return;
        };
        // A click in our own window: the window itself is the focused surface, so the
        // QuickClipWindow path needs no launcher token.
        if let Err(code) = request(&core, action, None) {
            core.update_ui(|u| u.message = request_error_text(code).into());
        }
    }

    pub fn toggle_pause(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            clipboard_runtime::toggle_pause(&core);
        }
    }

    pub fn dismiss_notice(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            clipboard_runtime::dismiss_notice(&core);
        }
    }
}

/// Text for the error codes of `clipboard_actions::request`.
pub fn request_error_text(code: &str) -> &'static str {
    match code {
        "signed-out" => "Sign in first.",
        "not-ready" => "The clipboard is not ready yet.",
        "nothing-to-send" => "Nothing to send yet: copy something first.",
        "nothing-to-paste" => "Nothing to paste.",
        _ => "The clipboard could not be written.",
    }
}
