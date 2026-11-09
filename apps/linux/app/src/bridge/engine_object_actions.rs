//! Invokable bodies of `EngineObject`: thin adapters from QML actions to `crate::engine`.

use super::engine_object::qobject;
use crate::core_state;
use crate::engine::{
    autostart_toggle, ops_account, ops_logout, ops_recovery, ops_session, shutdown,
};
use core::pin::Pin;
use cxx_qt_lib::QString;
use std::sync::atomic::Ordering;

impl qobject::EngineObject {
    pub fn login_start(self: Pin<&mut Self>, email: &QString) {
        if let Some(core) = core_state::get() {
            ops_session::login_start(&core, String::from(email));
        }
    }

    pub fn login_verify(self: Pin<&mut Self>, code: &QString) {
        if let Some(core) = core_state::get() {
            ops_session::login_verify(&core, String::from(code));
        }
    }

    pub fn back_to_email(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            ops_session::back_to_email(&core);
        }
    }

    pub fn logout(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            ops_logout::logout(&core);
        }
    }

    pub fn retry(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            ops_recovery::retry(&core);
        }
    }

    pub fn unlock_keyring(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            ops_recovery::unlock_keyring(&core);
        }
    }

    pub fn reset_history(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            ops_recovery::reset_history(&core);
        }
    }

    pub fn confirm_trust_reset(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            ops_account::answer_trust_reset(&core, true);
        }
    }

    pub fn decline_trust_reset(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            ops_account::answer_trust_reset(&core, false);
        }
    }

    pub fn set_autostart_enabled(self: Pin<&mut Self>, enabled: bool) {
        if let Some(core) = core_state::get() {
            autostart_toggle::set(&core, enabled);
            self.refresh();
        }
    }

    pub fn request_quit(self: Pin<&mut Self>) {
        shutdown::begin(core_state::get());
    }

    pub fn window_close_hides(&self) -> bool {
        core_state::get().is_some_and(|c| c.tray_registered.load(Ordering::SeqCst))
    }
}
