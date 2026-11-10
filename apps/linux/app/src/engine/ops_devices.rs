//! Device list: refresh, remove, live status patches.

use super::ops::{service_or_notify, show_error, spawn};
use crate::core_state::gate::OpKind;
use crate::core_state::ui_state::DeviceRow;
use crate::core_state::{Core, lock};
use fsync_core::service::{Device, FsyncService};
use std::sync::Arc;

pub fn refresh_blocking(core: &Core, svc: &FsyncService) {
    let generation = core.gate.begin(OpKind::Devices);
    let rows: Vec<DeviceRow> = svc.devices().iter().map(DeviceRow::from).collect();
    if core.gate.is_current(OpKind::Devices, generation) {
        core.set_devices(rows);
    }
}

pub fn refresh(core: &Arc<Core>) {
    let Some(svc) = core.service() else { return };
    spawn(core, "devices", move |c| refresh_blocking(c, &svc));
}

pub fn remove(core: &Arc<Core>, node_id: String) {
    let Some(svc) = service_or_notify(core) else {
        return;
    };
    spawn(core, "remove-device", move |c| {
        match svc.remove_device(&node_id) {
            Ok(()) => {}
            Err(e) => show_error(c, &e),
        }
        refresh_blocking(c, &svc);
    });
}

/// Presence changed for one device: patch the existing row under one lock. Unknown devices are
/// not inserted (the list refresh that follows `ListAdvanced` owns membership), and nothing is
/// patched while signed out (a late event must not resurrect a cleared list).
pub fn on_status(core: &Core, device: &Device) {
    if !lock(&core.ui).state.is_signed_in() {
        return;
    }
    let row = DeviceRow::from(device);
    let patched = {
        let mut rows = lock(&core.devices);
        rows.iter_mut()
            .find(|r| r.node_id == row.node_id)
            .map(|existing| *existing = row)
            .is_some()
    };
    if patched {
        core.hub.poke_devices();
    }
}
