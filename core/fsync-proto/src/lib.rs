//! Wire types shared by core and backend. Versioned by ALPN; never mutate v1 semantics.
//! No I/O in this crate.

pub mod api_dto;
pub mod clip;
pub mod device_list;
pub mod error;
pub mod file;
pub mod fixed_bytes;
pub mod frame;
pub mod names;
pub mod node_id;
pub mod pairing;
pub mod presence;
pub mod shape;
pub mod sig;
pub mod verify;

pub use clip::{
    ALPN_CLIP, ALPN_FILE, ALPN_PAIR, CLIP_ACK_ACCEPTED, CLIP_ACK_RATE_LIMITED, CLIP_ACK_REJECTED,
    ClipKind, ClipOffer, FileMeta, RepMeta,
};
pub use device_list::{DeviceEntry, DeviceList, Platform, SignedDeviceList, sign};
pub use error::ProtoError;
pub use file::{FetchReq, FetchResp, FetchStatus, FetchTarget};
pub use fixed_bytes::{Nonce, Sig};
pub use node_id::NodeId;
pub use pairing::{PairFrame, QrPayload};
pub use verify::{Outcome, verify_chain, verify_chain_from_genesis, verify_genesis, verify_next};
