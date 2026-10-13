//! Structural checks of a `DeviceList`, independent of any pinned state.

use crate::{
    device_list::{DEVLIST_VERSION, DeviceList},
    error::ProtoError,
    names::validate_device_name,
};

pub const MAX_DEVICES: usize = 32;

impl DeviceList {
    /// Version, 1..=32 unique devices, valid names. Run by `verify_genesis` / `verify_next`,
    /// so every verifier (backend and core) enforces the same caps.
    pub fn check_shape(&self) -> Result<(), ProtoError> {
        if self.v != DEVLIST_VERSION {
            return Err(ProtoError::UnsupportedVersion(self.v));
        }
        if self.devices.is_empty() {
            return Err(ProtoError::EmptyList);
        }
        if self.devices.len() > MAX_DEVICES {
            return Err(ProtoError::InvalidDeviceList("too many devices"));
        }
        for (i, d) in self.devices.iter().enumerate() {
            if self.devices[..i].iter().any(|o| o.node_id == d.node_id) {
                return Err(ProtoError::DuplicateDevice);
            }
            validate_device_name(&d.name)?;
        }
        Ok(())
    }
}
