//! Device list: status, removal.

use super::{
    FsyncService,
    clips::now,
    convert::parse_node,
    delegate::Out,
    error::{FsyncError, SvcResult},
    inner::{Inner, lock},
    types::{Device, DeviceStatus},
};
use crate::{error::CoreError, sync::sync_device_list};
use fsync_proto::{DeviceList, NodeId, device_list::DeviceEntry, sign};
use std::sync::Arc;

impl Inner {
    fn device_row(&self, e: &DeviceEntry) -> Device {
        let status = lock(&self.live)
            .ok()
            .and_then(|l| l.engine.clone())
            .map_or(DeviceStatus::Offline, |en| {
                en.node().path_state(&e.node_id).into()
            });
        let seen = lock(&self.peers)
            .ok()
            .and_then(|m| m.get(&e.node_id).copied())
            .unwrap_or_default();
        Device {
            node_id: e.node_id.to_string(),
            name: e.name.clone(),
            platform: e.platform,
            status,
            online: seen.online || status != DeviceStatus::Offline,
            last_seen_ms: seen.last_seen_ms,
            is_self: e.node_id == self.me(),
        }
    }

    pub(super) fn devices(&self) -> Vec<Device> {
        self.trust
            .list()
            .map(|l| l.devices.iter().map(|e| self.device_row(e)).collect())
            .unwrap_or_default()
    }

    /// Tells the app about one device (no-op for ids that are not listed).
    pub(super) fn emit_device(&self, id: &NodeId) {
        let row = self
            .trust
            .list()
            .and_then(|l| l.devices.into_iter().find(|d| d.node_id == *id))
            .map(|e| self.device_row(&e));
        if let Some(d) = row {
            self.emit(Out::Device(d));
        }
    }

    pub(super) async fn remove_device(self: &Arc<Self>, id: &NodeId) -> SvcResult<()> {
        if *id == self.me() {
            return Err(FsyncError::invalid("cannot remove this device from itself"));
        }
        if !self.trust.is_trusted(id) {
            return Err(FsyncError::NotPaired);
        }
        let head = self
            .trust
            .head()
            .ok_or(FsyncError::invalid("no pinned device list"))?;
        let prev_seq = head.decode().map_err(CoreError::from)?.seq;
        let next = DeviceList::next_remove(&head, self.me(), id, now()).map_err(CoreError::from)?;
        let signed = sign(self.identity.signing_key(), &next).map_err(CoreError::from)?;
        match self.api.put_device_list(&signed, Some(prev_seq)).await {
            Ok(_) => {}
            Err(CoreError::Api(e)) if e.status == 409 => {
                let _ = sync_device_list(&self.api, &self.trust).await;
                return Err(FsyncError::invalid("device list changed; try again"));
            }
            Err(e) => return Err(e.into()),
        }
        sync_device_list(&self.api, &self.trust).await?;
        self.list_freshness.stamp();
        Ok(())
    }
}

impl FsyncService {
    pub fn devices(&self) -> Vec<Device> {
        self.inner.devices()
    }

    /// Signs the next device list without `node_id` and publishes it; the removed device is
    /// cut off everywhere once the list propagates.
    pub fn remove_device(&self, node_id: &str) -> SvcResult<()> {
        let id = parse_node(node_id)?;
        self.run(self.inner.remove_device(&id))
    }
}
