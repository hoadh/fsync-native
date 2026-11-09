//! Dispatch impls: protocol events are forwarded to `WlState` (`wayland_watch`) for both
//! data-control protocols through one macro.

use super::wayland_proto::Offer;
use super::wayland_watch::WlState;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::{self as ext_dev, ExtDataControlDeviceV1},
    ext_data_control_manager_v1::ExtDataControlManagerV1,
    ext_data_control_offer_v1::{self as ext_off, ExtDataControlOfferV1},
    ext_data_control_source_v1::{self as ext_src, ExtDataControlSourceV1},
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self as wlr_dev, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::{self as wlr_off, ZwlrDataControlOfferV1},
    zwlr_data_control_source_v1::{self as wlr_src, ZwlrDataControlSourceV1},
};

// The manager has no events; the seat (wl_seat) events are of no interest.
impl Dispatch<ExtDataControlManagerV1, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &ExtDataControlManagerV1,
        _: <ExtDataControlManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<ZwlrDataControlManagerV1, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &ZwlrDataControlManagerV1,
        _: <ZwlrDataControlManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<WlSeat, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: <WlSeat as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

macro_rules! forward_events {
    ($variant:ident, $Dev:ty, $dev:ident, $Off:ty, $off:ident, $Src:ty, $src:ident) => {
        impl Dispatch<$Dev, ()> for WlState {
            fn event(state: &mut Self, _: &$Dev, ev: $dev::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
                match ev {
                    $dev::Event::DataOffer { id } => state.on_data_offer(Offer::$variant(id)),
                    $dev::Event::Selection { id } => state.on_selection(id.map(Offer::$variant)),
                    $dev::Event::PrimarySelection { id } => state.on_primary(id.map(Offer::$variant)),
                    $dev::Event::Finished => state.on_finished(),
                    _ => {}
                }
            }
            event_created_child!(WlState, $Dev, [ $dev::EVT_DATA_OFFER_OPCODE => ($Off, ()) ]);
        }
        impl Dispatch<$Off, ()> for WlState {
            fn event(state: &mut Self, offer: &$Off, ev: $off::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
                if let $off::Event::Offer { mime_type } = ev {
                    state.on_offer_mime(offer.id(), mime_type);
                }
            }
        }
        impl Dispatch<$Src, ()> for WlState {
            fn event(state: &mut Self, source: &$Src, ev: $src::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
                match ev {
                    $src::Event::Send { mime_type, fd } => state.on_send(source.id(), mime_type, fd),
                    $src::Event::Cancelled => state.on_cancelled(source.id()),
                    _ => {}
                }
            }
        }
    };
}

forward_events!(
    Ext,
    ExtDataControlDeviceV1,
    ext_dev,
    ExtDataControlOfferV1,
    ext_off,
    ExtDataControlSourceV1,
    ext_src
);
forward_events!(
    Wlr,
    ZwlrDataControlDeviceV1,
    wlr_dev,
    ZwlrDataControlOfferV1,
    wlr_off,
    ZwlrDataControlSourceV1,
    wlr_src
);
