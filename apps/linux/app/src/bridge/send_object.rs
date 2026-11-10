//! `SendObject`: QML singleton behind the DevicePicker and the drop areas. It shows the staged
//! set (summary, stage id) and answers it; all checks and the send run in `engine::send_files`.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qlist.h");
        type QList_QUrl = cxx_qt_lib::QList<cxx_qt_lib::QUrl>;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // What is staged ("3 items: a, b, c (1.2 MiB)"); empty when nothing is.
        #[qproperty(QString, summary)]
        // Id of the shown set; empty when nothing is staged (the picker closes then).
        #[qproperty(QString, stage_id, cxx_name = "stageId")]
        type SendObject = super::SendObjectRust;

        /// DevicePicker answer for the set with id `stage_id` (the one on display).
        #[qinvokable]
        #[cxx_name = "pickDevice"]
        fn pick_device(self: Pin<&mut SendObject>, node_id: &QString, stage_id: &QString);
        #[qinvokable]
        #[cxx_name = "cancelPick"]
        fn cancel_pick(self: Pin<&mut SendObject>);
        /// Files dropped on one device row (sent at once).
        #[qinvokable]
        #[cxx_name = "dropOnDevice"]
        fn drop_on_device(self: Pin<&mut SendObject>, node_id: &QString, urls: &QList_QUrl);
        /// Files dropped on the window: the picker opens once they pass the checks.
        #[qinvokable]
        #[cxx_name = "dropAnywhere"]
        fn drop_anywhere(self: Pin<&mut SendObject>, urls: &QList_QUrl);
    }

    impl cxx_qt::Threading for SendObject {}
    impl cxx_qt::Initialize for SendObject {}
}

use crate::core_state::{self, lock};
use crate::engine::send_files;
use core::pin::Pin;
use cxx_qt::Threading;
use cxx_qt_lib::{QList, QString, QUrl};

#[derive(Default)]
pub struct SendObjectRust {
    summary: QString,
    stage_id: QString,
}

impl cxx_qt::Initialize for qobject::SendObject {
    fn initialize(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            core.hub.register_send(self.qt_thread());
        }
        self.refresh();
    }
}

/// Encoded URLs, one per line (`to_encoded` keeps spaces and `#`/`%` in names unambiguous; the
/// engine decodes once).
fn encoded_lines(urls: &QList<QUrl>) -> String {
    urls.iter()
        .map(|u| String::from_utf8_lossy(u.to_encoded().as_ref()).into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

impl qobject::SendObject {
    /// Copies the staged set into the properties (GUI thread only).
    pub fn refresh(mut self: Pin<&mut Self>) {
        let Some(core) = core_state::get() else {
            return;
        };
        let (id, summary) = {
            let s = lock(&core.pending_send);
            let id = if s.is_empty() {
                String::new()
            } else {
                s.id().to_string()
            };
            (id, s.summary.clone())
        };
        self.as_mut().set_summary(QString::from(&summary));
        self.set_stage_id(QString::from(&id));
    }

    pub fn pick_device(self: Pin<&mut Self>, node_id: &QString, stage_id: &QString) {
        let Some(core) = core_state::get() else {
            return;
        };
        let shown = String::from(stage_id).parse::<u64>().ok();
        let result = match shown {
            Some(id) => send_files::pick(&core, &String::from(node_id), Some(id)),
            None => Err(send_files::NOTHING_STAGED),
        };
        if let Err(code) = result {
            core.update_ui(|u| u.message = send_files::refusal_text(code).into());
        }
    }

    pub fn cancel_pick(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            send_files::cancel_pending(&core);
        }
    }

    pub fn drop_on_device(self: Pin<&mut Self>, node_id: &QString, urls: &QList<QUrl>) {
        if let Some(core) = core_state::get() {
            send_files::dropped(&core, encoded_lines(urls), Some(String::from(node_id)));
        }
    }

    pub fn drop_anywhere(self: Pin<&mut Self>, urls: &QList<QUrl>) {
        if let Some(core) = core_state::get() {
            send_files::dropped(&core, encoded_lines(urls), None);
        }
    }
}
