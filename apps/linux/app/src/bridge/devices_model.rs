//! `DevicesModel`: QAbstractListModel singleton over the shared device snapshot. Rows are
//! reloaded as a whole (device lists are small); `removeDevice` runs on a worker thread.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++Qt" {
        include!(<QtCore/QAbstractListModel>);
        #[qobject]
        type QAbstractListModel;
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
    }

    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(i32, count)]
        type DevicesModel = super::DevicesModelRust;

        #[qinvokable]
        #[cxx_name = "removeDevice"]
        fn remove_device(self: Pin<&mut DevicesModel>, node_id: &QString);
        #[qinvokable]
        fn refresh(self: Pin<&mut DevicesModel>);
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &DevicesModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &DevicesModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &DevicesModel, parent: &QModelIndex) -> i32;
    }

    unsafe extern "RustQt" {
        /// # Safety
        /// Every `begin_reset_model` must be followed by `end_reset_model`.
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut DevicesModel>);
        /// # Safety
        /// Only after `begin_reset_model`.
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut DevicesModel>);
    }

    impl cxx_qt::Threading for DevicesModel {}
    impl cxx_qt::Initialize for DevicesModel {}
}

use crate::core_state::ui_state::DeviceRow;
use crate::core_state::{self, lock};
use crate::engine::ops_devices;
use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};

/// `Qt::UserRole`: custom roles must start here, lower values are Qt's own (display, decoration...).
const USER_ROLE: i32 = 0x0100;
const ROLE_NAME: i32 = 0;
const ROLE_PLATFORM: i32 = 1;
const ROLE_STATUS: i32 = 2;
const ROLE_NODE_ID: i32 = 3;
const ROLE_IS_SELF: i32 = 4;

#[derive(Default)]
pub struct DevicesModelRust {
    count: i32,
    rows: Vec<DeviceRow>,
}

impl cxx_qt::Initialize for qobject::DevicesModel {
    fn initialize(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            core.hub.register_devices(self.qt_thread());
        }
        self.reload();
    }
}

impl qobject::DevicesModel {
    /// Replaces all rows from the shared snapshot (GUI thread only).
    pub fn reload(mut self: Pin<&mut Self>) {
        let rows = core_state::get()
            .map(|c| lock(&c.devices).clone())
            .unwrap_or_default();
        let count = rows.len() as i32;
        // SAFETY: end_reset_model follows begin_reset_model on the same thread, nothing between.
        unsafe {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().rows = rows;
            self.as_mut().end_reset_model();
        }
        self.set_count(count);
    }

    pub fn refresh(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            ops_devices::refresh(&core);
        }
    }

    pub fn remove_device(self: Pin<&mut Self>, node_id: &QString) {
        if let Some(core) = core_state::get() {
            ops_devices::remove(&core, String::from(node_id));
        }
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Ok(row) = usize::try_from(index.row()) else {
            return QVariant::default();
        };
        let Some(d) = self.rows.get(row) else {
            return QVariant::default();
        };
        match role - USER_ROLE {
            ROLE_NAME => QVariant::from(&QString::from(&d.name)),
            ROLE_PLATFORM => QVariant::from(&QString::from(d.platform)),
            ROLE_STATUS => QVariant::from(&QString::from(d.status)),
            ROLE_NODE_ID => QVariant::from(&QString::from(&d.node_id)),
            ROLE_IS_SELF => QVariant::from(&d.is_self),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        for (offset, name) in [
            (ROLE_NAME, "name"),
            (ROLE_PLATFORM, "platform"),
            (ROLE_STATUS, "status"),
            (ROLE_NODE_ID, "nodeId"),
            (ROLE_IS_SELF, "isSelf"),
        ] {
            roles.insert(USER_ROLE + offset, QByteArray::from(name));
        }
        roles
    }

    pub fn row_count(&self, parent: &QModelIndex) -> i32 {
        // A flat list: only the invalid (root) parent has children.
        if parent.is_valid() {
            return 0;
        }
        self.rows.len() as i32
    }
}
