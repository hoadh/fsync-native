//! Helpers of the existing-device handshake (list publication, own entry, frame reads).

use super::{ListBackend, STEP_TIMEOUT, now_ms, wire};
use crate::{
    error::{CoreError, PairError, Result},
    identity::Identity,
    trust::TrustManager,
};
use fsync_proto::{DeviceList, NodeId, PairFrame, Platform, SignedDeviceList, sign};
use iroh::endpoint::RecvStream;

/// This device's own name/platform as recorded in the pinned list.
pub(super) fn own_entry(trust: &TrustManager, me: &NodeId) -> Result<(String, Platform)> {
    let list = trust.list().ok_or(PairError::NoPinnedList)?;
    list.devices
        .iter()
        .find(|d| d.node_id == *me)
        .map(|d| (d.name.clone(), d.platform))
        .ok_or_else(|| PairError::NoPinnedList.into())
}

/// The pinned head when `node` is already a member of the pinned list.
pub(super) fn listed_head(trust: &TrustManager, node: &NodeId) -> Option<SignedDeviceList> {
    trust.list()?.contains(node).then(|| trust.head())?
}

/// Build, sign and PUT `list + new device`; on a 409 re-sync and retry (loser reapplies).
pub(super) async fn publish_new_list<B: ListBackend>(
    me: &Identity,
    trust: &TrustManager,
    backend: &B,
    new: NodeId,
    name: String,
    platform: Platform,
) -> Result<SignedDeviceList> {
    for _ in 0..3 {
        let head = trust.head().ok_or(PairError::NoPinnedList)?;
        let prev_seq = head.decode()?.seq;
        let list =
            DeviceList::next_add(&head, me.node_id(), new, name.clone(), platform, now_ms())?;
        let signed = sign(me.signing_key(), &list)?;
        match backend.publish(&signed, prev_seq).await {
            Ok(()) => return Ok(signed),
            Err(CoreError::Api(a)) if a.status == 409 => backend.sync().await?,
            Err(e) => return Err(e),
        }
    }
    Err(CoreError::Invalid("device list kept changing; retry"))
}

pub(super) async fn recv_step(rx: &mut RecvStream) -> Result<PairFrame> {
    match wire::recv(rx, STEP_TIMEOUT).await? {
        PairFrame::Abort { reason } => Err(PairError::PeerAborted(reason).into()),
        f => Ok(f),
    }
}

pub(super) fn zeroizing_vec(b: &[u8]) -> zeroize::Zeroizing<Vec<u8>> {
    zeroize::Zeroizing::new(b.to_vec())
}
