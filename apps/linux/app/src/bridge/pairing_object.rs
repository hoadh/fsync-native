//! `PairingObject`: QML singleton for the pairing flow (show code, show QR, join, SAS).

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
        #[qproperty(QString, phase)]
        #[qproperty(QString, code)]
        #[qproperty(QString, qr_url, cxx_name = "qrUrl")]
        #[qproperty(QString, qr_text, cxx_name = "qrText")]
        #[qproperty(QString, sas)]
        /// Id of the confirmation on screen, passed back by `confirm` (a string: u64 exceeds JS numbers).
        #[qproperty(QString, request_id, cxx_name = "requestId")]
        #[qproperty(QString, peer_name, cxx_name = "peerName")]
        #[qproperty(QString, message)]
        type PairingObject = super::PairingObjectRust;

        #[qinvokable]
        #[cxx_name = "offerCode"]
        fn offer_code(self: Pin<&mut PairingObject>);
        #[qinvokable]
        #[cxx_name = "offerQr"]
        fn offer_qr(self: Pin<&mut PairingObject>);
        #[qinvokable]
        fn join(self: Pin<&mut PairingObject>, input: &QString);
        /// Answer the SAS question for the request the dialog rendered (`request_id`).
        #[qinvokable]
        fn confirm(self: Pin<&mut PairingObject>, request_id: &QString, accept: bool);
        /// Abandons the offer or join (direct control call, never queued behind a long op).
        #[qinvokable]
        fn cancel(self: Pin<&mut PairingObject>);
        /// Back to the idle page after done/failed.
        #[qinvokable]
        fn reset(self: Pin<&mut PairingObject>);
    }

    impl cxx_qt::Threading for PairingObject {}
    impl cxx_qt::Initialize for PairingObject {}
}

use crate::core_state::{self, lock};
use crate::engine::ops_pairing;
use core::pin::Pin;
use cxx_qt::Threading;
use cxx_qt_lib::QString;
use fsync_core::service::PairingMode;

#[derive(Default)]
pub struct PairingObjectRust {
    phase: QString,
    code: QString,
    qr_url: QString,
    qr_text: QString,
    sas: QString,
    request_id: QString,
    peer_name: QString,
    message: QString,
}

impl cxx_qt::Initialize for qobject::PairingObject {
    fn initialize(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            core.hub.register_pairing(self.qt_thread());
        }
        self.refresh();
    }
}

impl qobject::PairingObject {
    pub fn refresh(mut self: Pin<&mut Self>) {
        let Some(core) = core_state::get() else {
            return;
        };
        let p = lock(&core.pairing).clone();
        self.as_mut().set_phase(QString::from(p.phase.as_str()));
        self.as_mut().set_code(QString::from(&p.code));
        self.as_mut().set_qr_url(QString::from(&p.qr_url));
        self.as_mut().set_qr_text(QString::from(&p.qr_text));
        self.as_mut().set_sas(QString::from(&p.sas));
        self.as_mut()
            .set_request_id(QString::from(&p.request_id.to_string()));
        self.as_mut().set_peer_name(QString::from(&p.peer_name));
        self.as_mut().set_message(QString::from(&p.message));
    }

    pub fn offer_code(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            ops_pairing::offer(&core, PairingMode::Code);
        }
    }

    pub fn offer_qr(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            ops_pairing::offer(&core, PairingMode::Qr);
        }
    }

    pub fn join(self: Pin<&mut Self>, input: &QString) {
        if let Some(core) = core_state::get() {
            ops_pairing::join(&core, String::from(input));
        }
    }

    pub fn confirm(self: Pin<&mut Self>, request_id: &QString, accept: bool) {
        if let Some(core) = core_state::get() {
            ops_pairing::confirm(&core, &String::from(request_id), accept);
        }
    }

    pub fn cancel(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            ops_pairing::cancel(&core);
        }
    }

    pub fn reset(self: Pin<&mut Self>) {
        if let Some(core) = core_state::get() {
            ops_pairing::reset(&core);
        }
    }
}
