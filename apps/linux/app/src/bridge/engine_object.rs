//! `EngineObject`: QML singleton with the account/engine state and the sign-in, sign-out,
//! trust-reset and quit actions. All real work is in `crate::engine`; this only adapts types.

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
        #[qproperty(QString, state)]
        #[qproperty(QString, email)]
        #[qproperty(QString, node_id, cxx_name = "nodeId")]
        #[qproperty(QString, banner)]
        #[qproperty(QString, message)]
        #[qproperty(bool, busy)]
        #[qproperty(QString, trust_reset_id, cxx_name = "trustResetId")]
        #[qproperty(QString, trust_reset_names, cxx_name = "trustResetNames")]
        #[qproperty(bool, can_unlock_keyring, cxx_name = "canUnlockKeyring")]
        #[qproperty(bool, can_reset_history, cxx_name = "canResetHistory")]
        #[qproperty(bool, autostart)]
        #[qproperty(bool, tray_available, cxx_name = "trayAvailable")]
        type EngineObject = super::EngineObjectRust;

        /// Raise the main window (`token`: xdg-activation token, may be empty).
        #[qsignal]
        #[cxx_name = "showWindowRequested"]
        fn show_window_requested(self: Pin<&mut EngineObject>, token: QString);

        /// Show one section of the signed-in view ("history", "offers").
        #[qsignal]
        #[cxx_name = "sectionRequested"]
        fn section_requested(self: Pin<&mut EngineObject>, section: QString);

        #[qsignal]
        #[cxx_name = "toggleWindowRequested"]
        fn toggle_window_requested(self: Pin<&mut EngineObject>);

        /// Shutdown finished: QML quits the application.
        #[qsignal]
        #[cxx_name = "quitRequested"]
        fn quit_requested(self: Pin<&mut EngineObject>);

        #[qinvokable]
        #[cxx_name = "loginStart"]
        fn login_start(self: Pin<&mut EngineObject>, email: &QString);
        #[qinvokable]
        #[cxx_name = "loginVerify"]
        fn login_verify(self: Pin<&mut EngineObject>, code: &QString);
        #[qinvokable]
        #[cxx_name = "backToEmail"]
        fn back_to_email(self: Pin<&mut EngineObject>);
        #[qinvokable]
        fn logout(self: Pin<&mut EngineObject>);
        /// "Try again": re-runs boot when it failed, else retries going online.
        #[qinvokable]
        fn retry(self: Pin<&mut EngineObject>);
        /// "Unlock keyring": allows one more unlock prompt and re-checks at once.
        #[qinvokable]
        #[cxx_name = "unlockKeyring"]
        fn unlock_keyring(self: Pin<&mut EngineObject>);
        /// "Reset history" after a history key mismatch (core moves the old history aside).
        #[qinvokable]
        #[cxx_name = "resetHistory"]
        fn reset_history(self: Pin<&mut EngineObject>);
        #[qinvokable]
        #[cxx_name = "confirmTrustReset"]
        fn confirm_trust_reset(self: Pin<&mut EngineObject>);
        #[qinvokable]
        #[cxx_name = "declineTrustReset"]
        fn decline_trust_reset(self: Pin<&mut EngineObject>);
        #[qinvokable]
        #[cxx_name = "setAutostartEnabled"]
        fn set_autostart_enabled(self: Pin<&mut EngineObject>, enabled: bool);
        #[qinvokable]
        #[cxx_name = "requestQuit"]
        fn request_quit(self: Pin<&mut EngineObject>);
        /// Whether closing the window may hide it (a tray icon can bring it back).
        #[qinvokable]
        #[cxx_name = "windowCloseHides"]
        fn window_close_hides(self: &EngineObject) -> bool;
    }

    impl cxx_qt::Threading for EngineObject {}
    impl cxx_qt::Initialize for EngineObject {}
}

use super::app_identity;
use crate::core_state::ui_state::EngineState;
use crate::core_state::{self, lock};
use crate::engine::autostart_toggle;
use core::pin::Pin;
use cxx_qt::Threading;
use cxx_qt_lib::QString;
use std::sync::atomic::Ordering;

#[derive(Default)]
pub struct EngineObjectRust {
    state: QString,
    email: QString,
    node_id: QString,
    banner: QString,
    message: QString,
    busy: bool,
    can_unlock_keyring: bool,
    can_reset_history: bool,
    trust_reset_id: QString,
    trust_reset_names: QString,
    autostart: bool,
    tray_available: bool,
}

impl cxx_qt::Initialize for qobject::EngineObject {
    fn initialize(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            core.hub.register_engine(self.qt_thread());
        }
        self.refresh();
    }
}

impl qobject::EngineObject {
    /// Copies the shared UI snapshot into the properties (GUI thread only).
    pub fn refresh(mut self: Pin<&mut Self>) {
        let Some(core) = core_state::get() else {
            return;
        };
        let ui = lock(&core.ui).clone();
        let booting = matches!(
            ui.state,
            EngineState::Starting | EngineState::WaitingForKeyring
        );
        let (reset_id, names) = ui.trust_reset.clone().unwrap_or_default();
        self.as_mut().set_state(QString::from(ui.state.as_str()));
        self.as_mut().set_email(QString::from(&ui.email));
        self.as_mut().set_node_id(QString::from(&ui.node_id));
        self.as_mut()
            .set_banner(QString::from(&ui.warnings.banner(booting)));
        self.as_mut().set_message(QString::from(&ui.message));
        self.as_mut().set_busy(ui.busy);
        self.as_mut()
            .set_can_unlock_keyring(ui.warnings.can_unlock());
        self.as_mut().set_can_reset_history(ui.history_mismatch);
        self.as_mut().set_trust_reset_id(QString::from(&reset_id));
        self.as_mut()
            .set_trust_reset_names(QString::from(&names.join(", ")));
        self.as_mut()
            .set_autostart(autostart_toggle::is_enabled(&core));
        self.as_mut()
            .set_tray_available(core.tray_registered.load(Ordering::SeqCst));
    }

    /// Raises the window. The signal handler (QML) runs synchronously, so the activation token
    /// is in the environment exactly while `requestActivate` runs.
    pub fn emit_show_window(self: Pin<&mut Self>, token: QString) {
        let token_text = String::from(&token);
        let have_token = app_identity::set_activation_token(&token_text);
        self.show_window_requested(token);
        if have_token {
            app_identity::clear_activation_token();
        }
    }

    pub fn emit_section(self: Pin<&mut Self>, section: QString) {
        self.section_requested(section);
    }

    pub fn emit_toggle_window(self: Pin<&mut Self>) {
        self.toggle_window_requested();
    }

    pub fn emit_quit(self: Pin<&mut Self>) {
        self.quit_requested();
    }
}
