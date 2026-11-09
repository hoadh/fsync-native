//! ext-data-control-v1 and zwlr-data-control-v1 have the same shape; this module wraps both
//! behind one set of enums so the watch/serve logic is written once. Events are forwarded to
//! `WlState` methods (`wayland_dispatch`). Only the regular selection is used: primary selection
//! offers are destroyed on sight (decided: primary is ignored).

use super::wayland_watch::WlState;
use std::os::fd::BorrowedFd;
use wayland_client::backend::ObjectId;
use wayland_client::globals::GlobalList;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Proxy, QueueHandle};

use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::ExtDataControlDeviceV1,
    ext_data_control_manager_v1::ExtDataControlManagerV1,
    ext_data_control_offer_v1::ExtDataControlOfferV1,
    ext_data_control_source_v1::ExtDataControlSourceV1,
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::ZwlrDataControlDeviceV1,
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::ZwlrDataControlOfferV1,
    zwlr_data_control_source_v1::ZwlrDataControlSourceV1,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Ext,
    Wlr,
}

pub const EXT_IFACE: &str = "ext_data_control_manager_v1";
pub const WLR_IFACE: &str = "zwlr_data_control_manager_v1";

/// Which data-control protocol the compositor offers (ext preferred).
pub fn protocol_in(globals: &GlobalList) -> Option<Protocol> {
    globals.contents().with_list(|list| {
        let has = |name: &str| list.iter().any(|g| g.interface == name);
        if has(EXT_IFACE) {
            Some(Protocol::Ext)
        } else if has(WLR_IFACE) {
            Some(Protocol::Wlr)
        } else {
            None
        }
    })
}

#[derive(Clone)]
pub enum Manager {
    Ext(ExtDataControlManagerV1),
    Wlr(ZwlrDataControlManagerV1),
}
#[derive(Clone)]
pub enum Device {
    Ext(ExtDataControlDeviceV1),
    Wlr(ZwlrDataControlDeviceV1),
}
#[derive(Clone)]
pub enum Offer {
    Ext(ExtDataControlOfferV1),
    Wlr(ZwlrDataControlOfferV1),
}
#[derive(Clone)]
pub enum Source {
    Ext(ExtDataControlSourceV1),
    Wlr(ZwlrDataControlSourceV1),
}

impl Manager {
    pub fn bind(
        globals: &GlobalList,
        qh: &QueueHandle<WlState>,
        p: Protocol,
    ) -> Result<Self, String> {
        let err = |e| format!("cannot bind data-control: {e}");
        Ok(match p {
            Protocol::Ext => Self::Ext(globals.bind(qh, 1..=1, ()).map_err(err)?),
            Protocol::Wlr => Self::Wlr(globals.bind(qh, 1..=1, ()).map_err(err)?),
        })
    }

    pub fn get_device(&self, seat: &WlSeat, qh: &QueueHandle<WlState>) -> Device {
        match self {
            Self::Ext(m) => Device::Ext(m.get_data_device(seat, qh, ())),
            Self::Wlr(m) => Device::Wlr(m.get_data_device(seat, qh, ())),
        }
    }

    pub fn create_source(&self, qh: &QueueHandle<WlState>) -> Source {
        match self {
            Self::Ext(m) => Source::Ext(m.create_data_source(qh, ())),
            Self::Wlr(m) => Source::Wlr(m.create_data_source(qh, ())),
        }
    }
}

impl Device {
    pub fn set_selection(&self, source: Option<&Source>) {
        match (self, source) {
            (Self::Ext(d), Some(Source::Ext(s))) => d.set_selection(Some(s)),
            (Self::Wlr(d), Some(Source::Wlr(s))) => d.set_selection(Some(s)),
            (Self::Ext(d), None) => d.set_selection(None),
            (Self::Wlr(d), None) => d.set_selection(None),
            _ => {} // mixed protocols cannot happen: one manager per connection
        }
    }

    pub fn destroy(&self) {
        match self {
            Self::Ext(d) => d.destroy(),
            Self::Wlr(d) => d.destroy(),
        }
    }
}

impl Offer {
    pub fn id(&self) -> ObjectId {
        match self {
            Self::Ext(o) => o.id(),
            Self::Wlr(o) => o.id(),
        }
    }

    pub fn receive(&self, mime: &str, fd: BorrowedFd<'_>) {
        match self {
            Self::Ext(o) => o.receive(mime.to_owned(), fd),
            Self::Wlr(o) => o.receive(mime.to_owned(), fd),
        }
    }

    pub fn destroy(&self) {
        match self {
            Self::Ext(o) => o.destroy(),
            Self::Wlr(o) => o.destroy(),
        }
    }
}

impl Source {
    pub fn id(&self) -> ObjectId {
        match self {
            Self::Ext(s) => s.id(),
            Self::Wlr(s) => s.id(),
        }
    }

    pub fn offer(&self, mime: &str) {
        match self {
            Self::Ext(s) => s.offer(mime.to_owned()),
            Self::Wlr(s) => s.offer(mime.to_owned()),
        }
    }

    pub fn destroy(&self) {
        match self {
            Self::Ext(s) => s.destroy(),
            Self::Wlr(s) => s.destroy(),
        }
    }
}
