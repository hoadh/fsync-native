//! `TransfersObject`: QAbstractListModel singleton with the pushed-file offers (from, count,
//! size, progress, state) and Accept / Decline / Cancel / Open folder / Dismiss. Work is in
//! `crate::engine::offers`.

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
        #[qproperty(i32, pending_count, cxx_name = "pendingCount")]
        type TransfersObject = super::TransfersObjectRust;

        #[qinvokable]
        fn accept(self: Pin<&mut TransfersObject>, item_id: &QString);
        #[qinvokable]
        fn decline(self: Pin<&mut TransfersObject>, item_id: &QString);
        #[qinvokable]
        fn cancel(self: Pin<&mut TransfersObject>, item_id: &QString);
        #[qinvokable]
        #[cxx_name = "openFolder"]
        fn open_folder(self: Pin<&mut TransfersObject>, item_id: &QString);
        #[qinvokable]
        fn dismiss(self: Pin<&mut TransfersObject>, item_id: &QString);
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &TransfersObject, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &TransfersObject) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &TransfersObject, parent: &QModelIndex) -> i32;
    }

    unsafe extern "RustQt" {
        /// Rows changed in place (same ids, same order): delegates and clicks survive.
        #[qsignal]
        #[inherit]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut TransfersObject>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QList_i32,
        );
        #[inherit]
        fn index(
            self: &TransfersObject,
            row: i32,
            column: i32,
            parent: &QModelIndex,
        ) -> QModelIndex;
        /// # Safety
        /// Every `begin_reset_model` must be followed by `end_reset_model`.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut TransfersObject>);
        /// # Safety
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut TransfersObject>);
    }

    impl cxx_qt::Threading for TransfersObject {}
    impl cxx_qt::Initialize for TransfersObject {}
}

use crate::core_state::files_state::{OfferRow, OfferState, lock};
use crate::core_state::{self, Core};
use crate::engine::offers;
use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{
    QByteArray, QHash, QHashPair_i32_QByteArray, QList, QModelIndex, QString, QVariant,
};
use std::sync::Arc;

/// `Qt::UserRole`: custom roles start here.
const USER_ROLE: i32 = 0x0100;
const ROLES: [&str; 8] = [
    "itemId",
    "fromName",
    "fileCount",
    "totalSize",
    "doneBytes",
    "offerState",
    "message",
    "dir",
];

#[derive(Default)]
pub struct TransfersObjectRust {
    count: i32,
    pending_count: i32,
    rows: Vec<OfferRow>,
}

impl cxx_qt::Initialize for qobject::TransfersObject {
    fn initialize(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            core.hub.register_offers(self.qt_thread());
        }
        self.reload();
    }
}

fn with_core(f: impl FnOnce(&Arc<Core>)) {
    if let Some(core) = core_state::get() {
        f(&core);
    }
}

impl qobject::TransfersObject {
    /// Replaces all rows from the shared snapshot (GUI thread only; a handful of rows).
    pub fn reload(mut self: Pin<&mut Self>) {
        let Some(core) = core_state::get() else {
            return;
        };
        let rows = lock(&core.files.offers).clone();
        let count = i32::try_from(rows.len()).unwrap_or(i32::MAX);
        let pending = rows
            .iter()
            .filter(|r| r.state == OfferState::Pending)
            .count();
        let same = self.rows.len() == rows.len()
            && self
                .rows
                .iter()
                .zip(&rows)
                .all(|(a, b)| a.item_id == b.item_id);
        if same && !rows.is_empty() {
            // Same offers in the same order: update in place so a click is not lost.
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
        self.set_pending_count(i32::try_from(pending).unwrap_or(i32::MAX));
    }

    pub fn accept(self: Pin<&mut Self>, item_id: &QString) {
        with_core(|c| offers::accept(c, String::from(item_id)));
    }

    pub fn decline(self: Pin<&mut Self>, item_id: &QString) {
        with_core(|c| offers::decline(c, &String::from(item_id)));
    }

    pub fn cancel(self: Pin<&mut Self>, item_id: &QString) {
        with_core(|c| offers::cancel(c, &String::from(item_id)));
    }

    pub fn open_folder(self: Pin<&mut Self>, item_id: &QString) {
        with_core(|c| offers::open_folder(c, &String::from(item_id)));
    }

    pub fn dismiss(self: Pin<&mut Self>, item_id: &QString) {
        with_core(|c| offers::dismiss(c, &String::from(item_id)));
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
            1 => text(&r.from_name),
            2 => QVariant::from(&i32::try_from(r.files.len()).unwrap_or(i32::MAX)),
            3 => QVariant::from(&(r.total as f64)),
            4 => QVariant::from(&(r.done_bytes() as f64)),
            5 => text(r.state.as_str()),
            6 => text(&r.message),
            7 => text(&r.dir),
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
