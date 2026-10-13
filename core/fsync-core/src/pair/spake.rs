//! SPAKE2 (Ed25519 group) over the 6-digit code. ids = (existing node id, new node id).

use crate::error::{PairError, Result};
use fsync_proto::NodeId;
use spake2::{Ed25519Group, Identity, Password, Spake2};
use zeroize::Zeroizing;

pub struct SpakeState(Spake2<Ed25519Group>);

fn ids(existing: &NodeId, new: &NodeId) -> (Identity, Identity) {
    (
        Identity::new(existing.as_bytes()),
        Identity::new(new.as_bytes()),
    )
}

/// Existing device = side A.
pub fn start_existing(code: &str, existing: &NodeId, new: &NodeId) -> (SpakeState, Vec<u8>) {
    let (a, b) = ids(existing, new);
    let (s, msg) = Spake2::<Ed25519Group>::start_a(&Password::new(code.as_bytes()), &a, &b);
    (SpakeState(s), msg)
}

/// New device = side B.
pub fn start_new(code: &str, existing: &NodeId, new: &NodeId) -> (SpakeState, Vec<u8>) {
    let (a, b) = ids(existing, new);
    let (s, msg) = Spake2::<Ed25519Group>::start_b(&Password::new(code.as_bytes()), &a, &b);
    (SpakeState(s), msg)
}

impl SpakeState {
    pub fn finish(self, peer_msg: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        self.0
            .finish(peer_msg)
            .map(Zeroizing::new)
            .map_err(|_| PairError::Protocol("bad spake2 message").into())
    }
}
