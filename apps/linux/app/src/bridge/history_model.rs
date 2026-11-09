//! `HistoryModel`: QAbstractListModel singleton over the History snapshot (newest first). Rows are
//! reloaded as a whole; every action runs on an op thread (`crate::engine::history_ops`).

#[cxx_qt::bridge]
pub mod qobject {
    // The base class is declared once (devices_model.rs): a second `extern "C++Qt"`
    // declaration would define its cast helpers twice at link time.
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel = crate::bridge::devices_model::qobject::QAbstractListModel;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qlist.h");
        type QList_i32 = cxx_qt_lib::QList<i32>;
    }

    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(i32, count)]
        #[qproperty(QString, message)]
        #[qproperty(bool, loading)]
        type HistoryModel = super::HistoryModelRust;

        /// Re-reads the list (the current search text applies).
        #[qinvokable]
        fn refresh(self: Pin<&mut HistoryModel>);
        #[qinvokable]
        fn search(self: Pin<&mut HistoryModel>, query: &QString);
        #[qinvokable]
        fn pin(self: Pin<&mut HistoryModel>, item_id: &QString, pinned: bool);
        #[qinvokable]
        #[cxx_name = "removeItem"]
        fn remove_item(self: Pin<&mut HistoryModel>, item_id: &QString);
        /// Clears the history but keeps pinned items.
        #[qinvokable]
        #[cxx_name = "clearUnpinned"]
        fn clear_unpinned(self: Pin<&mut HistoryModel>);
        /// Copy-back onto the clipboard.
        #[qinvokable]
        fn copy(self: Pin<&mut HistoryModel>, item_id: &QString);
        /// Downloads a large file clip, then opens its folder.
        #[qinvokable]
        fn download(self: Pin<&mut HistoryModel>, item_id: &QString);
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &HistoryModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &HistoryModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &HistoryModel, parent: &QModelIndex) -> i32;
    }

    unsafe extern "RustQt" {
        /// Rows changed in place (same ids, same order): delegates and clicks survive.
        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut HistoryModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QList_i32,
        );
        #[inherit]
        fn index(self: &HistoryModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
        /// # Safety
        /// Every `begin_reset_model` must be followed by `end_reset_model`.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut HistoryModel>);
        /// # Safety
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut HistoryModel>);
    }

    impl cxx_qt::Threading for HistoryModel {}
    impl cxx_qt::Initialize for HistoryModel {}
}

use super::clip_access::set_entry;
use crate::core_state::files_state::{HistoryRow, lock};
use crate::core_state::{self, Core};
use crate::engine::history_ops;
use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QList, QModelIndex, QString, QVariant,
};
use fsync_linux_platform::clipboard::OwnedEntry;
use std::sync::Arc;

/// `Qt::UserRole`: custom roles start here.
const USER_ROLE: i32 = 0x0100;
const ROLES: [&str; 11] = [
    "itemId",
    "kind",
    "preview",
    "peerName",
    "createdMs",
    "totalSize",
    "pinned",
    "outgoing",
    "canCopy",
    "canDownload",
    "downloading",
];

#[derive(Default)]
pub struct HistoryModelRust {
    count: i32,
    message: QString,
    loading: bool,
    rows: Vec<HistoryRow>,
}

impl cxx_qt::Initialize for qobject::HistoryModel {
    fn initialize(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            core.hub.register_history(self.qt_thread());
        }
        self.reload();
    }
}

fn with_core(f: impl FnOnce(&Arc<Core>)) {
    if let Some(core) = core_state::get() {
        f(&core);
    }
}

impl qobject::HistoryModel {
    /// Replaces all rows from the shared snapshot (GUI thread only).
    pub fn reload(mut self: Pin<&mut Self>) {
        let Some(core) = core_state::get() else {
            return;
        };
        let (rows, message, loading) = {
            let h = lock(&core.files.history);
            (h.rows.clone(), h.message.clone(), h.loading)
        };
        let count = i32::try_from(rows.len()).unwrap_or(i32::MAX);
        let same = self.rows.len() == rows.len()
            && self
                .rows
                .iter()
                .zip(&rows)
                .all(|(a, b)| a.item_id == b.item_id);
        if same && !rows.is_empty() {
            self.as_mut().rust_mut().rows = rows;
            let first = self.index(0, 0, &QModelIndex::default());
            let last = self.index(count - 1, 0, &QModelIndex::default());
            self.as_mut().data_changed(&first, &last, &QList::default());
        } else {
            // SAFETY: end_reset_model follows begin_reset_model on the same thread.
            unsafe {
                self.as_mut().begin_reset_model();
                self.as_mut().rust_mut().rows = rows;
                self.as_mut().end_reset_model();
            }
        }
        self.as_mut().set_count(count);
        self.as_mut().set_message(QString::from(&message));
        self.set_loading(loading);
    }

    /// Writes a copy-back entry through Qt: the focused History window is the surface that may
    /// own the clipboard on GNOME (no background backend there).
    pub fn write_entry(self: Pin<&mut Self>, entry: &OwnedEntry) {
        let text = if set_entry(entry) {
            "Copied."
        } else {
            "The clipboard could not be written."
        };
        with_core(|c| lock(&c.files.history).message = text.into());
        self.reload();
    }

    pub fn refresh(self: Pin<&mut Self>) {
        with_core(history_ops::reload);
    }

    pub fn search(self: Pin<&mut Self>, query: &QString) {
        with_core(|c| history_ops::search(c, String::from(query)));
    }

    pub fn pin(self: Pin<&mut Self>, item_id: &QString, pinned: bool) {
        with_core(|c| history_ops::pin(c, String::from(item_id), pinned));
    }

    pub fn remove_item(self: Pin<&mut Self>, item_id: &QString) {
        with_core(|c| history_ops::delete(c, String::from(item_id)));
    }

    pub fn clear_unpinned(self: Pin<&mut Self>) {
        with_core(history_ops::clear_unpinned);
    }

    pub fn copy(self: Pin<&mut Self>, item_id: &QString) {
        with_core(|c| history_ops::copy(c, String::from(item_id)));
    }

    pub fn download(self: Pin<&mut Self>, item_id: &QString) {
        with_core(|c| history_ops::download(c, String::from(item_id)));
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Ok(row) = usize::try_from(index.row()) else {
            return QVariant::default();
        };
        let Some(r) = self.rows.get(row) else {
            return QVariant::default();
        };
        let text = |s: &str| QVariant::from(&QString::from(s));
        match role - USER_ROLE {
            0 => text(&r.item_id),
            1 => text(r.kind),
            2 => text(&r.preview),
            3 => text(&r.peer_name),
            // Doubles are exact up to 2^53: enough for milliseconds and byte counts.
            4 => QVariant::from(&(r.created_ms as f64)),
            5 => QVariant::from(&(r.total_size as f64)),
            6 => QVariant::from(&r.pinned),
            7 => QVariant::from(&r.outgoing),
            8 => QVariant::from(&r.can_copy),
            9 => QVariant::from(&r.can_download),
            10 => QVariant::from(&r.downloading),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        for (offset, name) in (0..).zip(ROLES) {
            roles.insert(USER_ROLE + offset, QByteArray::from(name));
        }
        roles
    }

    pub fn row_count(&self, parent: &QModelIndex) -> i32 {
        if parent.is_valid() {
            return 0;
        }
        i32::try_from(self.rows.len()).unwrap_or(i32::MAX)
    }
}
