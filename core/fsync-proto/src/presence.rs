//! WS /v1/presence messages (JSON, tagged by `type`).

use crate::{
    error::ProtoError,
    fixed_bytes::{Nonce, Sig},
    node_id::NodeId,
    sig::{sign_domain, verify_domain},
};
use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

pub const PRESENCE_DOMAIN: &[u8] = b"fsync-presence-v1";

/// Sign the server nonce with the device key, binding the socket to the device.
pub fn sign_challenge(key: &SigningKey, nonce: &Nonce) -> Sig {
    sign_domain(key, PRESENCE_DOMAIN, &nonce.0)
}

pub fn verify_challenge(node_id: &NodeId, nonce: &Nonce, sig: &Sig) -> Result<(), ProtoError> {
    verify_domain(node_id, PRESENCE_DOMAIN, &nonce.0, sig)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Addrs {
    pub relay_url: Option<String>,
    pub direct: Vec<SocketAddr>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerInfo {
    pub node_id: NodeId,
    pub online: bool,
    pub addrs: Addrs,
    pub last_seen_ms: u64,
}

/// Client -> server.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    Auth {
        access: String,
        node_id: NodeId,
        sig: Sig,
    },
    Hello {
        addrs: Addrs,
    },
    Heartbeat,
}

// Redact the access token.
impl std::fmt::Debug for ClientMsg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auth { node_id, .. } => {
                write!(f, "Auth {{ node_id: {node_id}, access: <redacted> }}")
            }
            Self::Hello { addrs } => f.debug_struct("Hello").field("addrs", addrs).finish(),
            Self::Heartbeat => f.write_str("Heartbeat"),
        }
    }
}

/// Server -> client. `Challenge` is sent first on connect.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    Challenge { nonce: Nonce },
    Peers { peers: Vec<PeerInfo> },
    PeerUpdate(PeerInfo),
    DeviceListChanged { seq: u64 },
}
