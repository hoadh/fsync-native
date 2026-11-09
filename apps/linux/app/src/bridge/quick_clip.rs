//! `QuickClip`: QML singleton behind the QuickClipWindow (GNOME manual path). `run` is called
//! by the window once it has the keyboard focus; it reads or writes the clipboard through Qt.

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
        type QuickClip = super::QuickClipRust;

        /// `action`: "send" or "paste". Returns the sentence the window shows.
        #[qinvokable]
        fn run(self: &QuickClip, action: &QString) -> QString;
    }
}

use crate::core_state;
use crate::engine::quick_clip_run;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct QuickClipRust;

impl qobject::QuickClip {
    pub fn run(&self, action: &QString) -> QString {
        let Some(core) = core_state::get() else {
            return QString::from("fsync is not ready yet.");
        };
        QString::from(&quick_clip_run::run(&core, &String::from(action)))
    }
}
