//! `SettingsObject`: QML singleton behind the Settings screen. `load` and `save` run core calls
//! on an op thread and come back through `CxxQtThread::queue`; the object never blocks the GUI.

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
        #[qproperty(bool, loaded)]
        #[qproperty(bool, busy)]
        #[qproperty(bool, manual)]
        #[qproperty(i32, retention_days, cxx_name = "retentionDays")]
        #[qproperty(i32, retention_mib, cxx_name = "retentionMib")]
        #[qproperty(i32, max_clip_mib, cxx_name = "maxClipMib")]
        #[qproperty(bool, auto_accept, cxx_name = "autoAccept")]
        #[qproperty(QString, denylist)]
        // Outcome of the last load / save, and whether it is a problem.
        #[qproperty(QString, status)]
        #[qproperty(bool, status_is_error, cxx_name = "statusIsError")]
        // Set when secrets are kept in a file instead of the system keyring.
        #[qproperty(QString, secrets_warning, cxx_name = "secretsWarning")]
        // JSON list of hotkey bind help (see `engine::shortcut_help`).
        #[qproperty(QString, shortcuts)]
        type SettingsObject = super::SettingsObjectRust;

        /// Reads core's settings (call when the screen is shown).
        #[qinvokable]
        fn load(self: Pin<&mut SettingsObject>);
        #[qinvokable]
        fn save(
            self: Pin<&mut SettingsObject>,
            manual: bool,
            retention_days: i32,
            retention_mib: i32,
            max_clip_mib: i32,
            auto_accept: bool,
            denylist: &QString,
        );
    }

    impl cxx_qt::Threading for SettingsObject {}
    impl cxx_qt::Initialize for SettingsObject {}
}

use crate::core_state::{self, lock};
use crate::engine::ops::spawn;
use crate::engine::settings_ops::{self, Form};
use crate::engine::shortcut_help;
use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct SettingsObjectRust {
    loaded: bool,
    busy: bool,
    manual: bool,
    retention_days: i32,
    retention_mib: i32,
    max_clip_mib: i32,
    auto_accept: bool,
    denylist: QString,
    status: QString,
    status_is_error: bool,
    secrets_warning: QString,
    shortcuts: QString,
    /// The values last loaded or saved: what an untouched field still holds.
    baseline: Option<Form>,
}

impl cxx_qt::Initialize for qobject::SettingsObject {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut()
            .set_shortcuts(QString::from(&shortcut_help::json()));
        // Same wording as the status banner: no keyring, secrets in a protected file.
        let file_mode = core_state::get().is_some_and(|c| lock(&c.ui).warnings.file_fallback);
        if file_mode {
            self.set_secrets_warning(QString::from(
                "No system keyring was found: your sign-in and keys are stored in a protected file in the app's data folder.",
            ));
        }
    }
}

/// `u64` MiB to the `i32` a QML SpinBox holds.
fn clamp_i32(v: u64) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

impl qobject::SettingsObject {
    /// GUI thread: shows `form`.
    fn show(mut self: Pin<&mut Self>, form: &Form) {
        self.as_mut().set_manual(form.manual);
        self.as_mut()
            .set_retention_days(clamp_i32(u64::from(form.retention_days)));
        self.as_mut()
            .set_retention_mib(clamp_i32(form.retention_mib));
        self.as_mut().set_max_clip_mib(clamp_i32(form.max_clip_mib));
        self.as_mut().set_auto_accept(form.auto_accept);
        self.as_mut().set_denylist(QString::from(&form.denylist));
        self.as_mut().rust_mut().baseline = Some(form.clone());
        self.as_mut().set_loaded(true);
    }

    fn finish(mut self: Pin<&mut Self>, status: &str, is_error: bool) {
        self.as_mut().set_busy(false);
        self.as_mut().set_status_is_error(is_error);
        self.set_status(QString::from(status));
    }

    pub fn load(self: Pin<&mut Self>) {
        let Some(core) = core_state::get() else {
            return;
        };
        let thread = self.qt_thread();
        spawn(&core, "settings-load", move |c| {
            let result = settings_ops::load(c);
            // A failed queue means the object is gone (app quitting).
            let _ = thread.queue(move |mut o| match result {
                Ok(form) => {
                    o.as_mut().show(&form);
                    o.finish("", false);
                }
                Err(e) => o.finish(&e, true),
            });
        });
    }

    pub fn save(
        mut self: Pin<&mut Self>,
        manual: bool,
        retention_days: i32,
        retention_mib: i32,
        max_clip_mib: i32,
        auto_accept: bool,
        denylist: &QString,
    ) {
        let Some(core) = core_state::get() else {
            return;
        };
        let Some(base) = self.as_ref().rust().baseline.clone() else {
            return; // never loaded: nothing to compare with, nothing to write
        };
        let form = Form {
            manual,
            // Negative values from a bad binding fail core validation as 0, never wrap.
            retention_days: u32::try_from(retention_days).unwrap_or(0),
            retention_mib: settings_ops::resolve_mib(i64::from(retention_mib), base.retention_mib),
            max_clip_mib: settings_ops::resolve_mib(i64::from(max_clip_mib), base.max_clip_mib),
            auto_accept,
            denylist: String::from(denylist),
        };
        self.as_mut().set_busy(true);
        self.as_mut().set_status(QString::default());
        let thread = self.qt_thread();
        spawn(&core, "settings-save", move |c| {
            let result = settings_ops::save(c, &form);
            let _ = thread.queue(move |mut o| match result {
                Ok(stored) => {
                    o.as_mut().show(&stored);
                    o.finish("Saved.", false);
                }
                Err(e) => o.finish(&e, true),
            });
        });
    }
}
