//! pair/1 frames and the QR payload (`fsync://pair?v=1&n=..&s=..&r=..`).

use crate::{
    device_list::{Platform, SignedDeviceList},
    error::ProtoError,
    node_id::NodeId,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use std::fmt;
use url::{Url, form_urlencoded};
use zeroize::{Zeroize, Zeroizing};

pub const PAIR_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PairMode {
    Code,
    Qr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbortReason {
    BadMac,
    Rejected,
    UnsupportedVersion,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PairFrame {
    Hello {
        v: u8,
        mode: PairMode,
        node_id: NodeId,
        /// Fresh per session; bound into the transcript so keys/MAC/SAS differ every time.
        nonce: [u8; 16],
    },
    /// New device's reply to `Hello`: its transcript nonce.
    HelloReply {
        nonce: [u8; 16],
    },
    Spake {
        msg: Vec<u8>,
    },
    Confirm {
        mac: [u8; 32],
    },
    DeviceInfo {
        name: String,
        platform: Platform,
    },
    /// New device's explicit local approval (after comparing the SAS); precedes `SignedList`.
    Accept,
    SignedList {
        chain: Vec<SignedDeviceList>,
    },
    Abort {
        reason: AbortReason,
    },
    Done,
}

/// Contents of the pairing QR code shown by the new device.
#[derive(Clone, PartialEq, Eq)]
pub struct QrPayload {
    pub node_id: NodeId,
    pub secret: [u8; 16],
    pub relay: Option<String>,
}

// Never print the secret.
impl fmt::Debug for QrPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QrPayload")
            .field("node_id", &self.node_id)
            .field("secret", &"<redacted>")
            .field("relay", &self.relay)
            .finish()
    }
}

impl Drop for QrPayload {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

impl QrPayload {
    pub fn build(&self) -> String {
        let mut q = form_urlencoded::Serializer::new(String::new());
        q.append_pair("v", &PAIR_VERSION.to_string());
        q.append_pair("n", &self.node_id.to_base32());
        // Best effort: the intermediate base64 is wiped; the returned string carries the secret.
        let b64 = Zeroizing::new(URL_SAFE_NO_PAD.encode(self.secret));
        q.append_pair("s", &b64);
        if let Some(r) = &self.relay {
            q.append_pair("r", r);
        }
        format!("fsync://pair?{}", q.finish())
    }

    pub fn parse(s: &str) -> Result<Self, ProtoError> {
        let url = Url::parse(s).map_err(|_| ProtoError::InvalidQr("not a url"))?;
        if url.scheme() != "fsync" || url.host_str() != Some("pair") {
            return Err(ProtoError::InvalidQr("wrong scheme or host"));
        }
        let (mut v, mut n, mut relay) = (None, None, None);
        let mut sec: Option<Zeroizing<String>> = None;
        for (k, val) in url.query_pairs() {
            if k == "s" {
                if sec.replace(Zeroizing::new(val.into_owned())).is_some() {
                    return Err(ProtoError::InvalidQr("duplicate parameter"));
                }
                continue;
            }
            let slot = match k.as_ref() {
                "v" => &mut v,
                "n" => &mut n,
                "r" => &mut relay,
                _ => continue,
            };
            if slot.replace(val.into_owned()).is_some() {
                return Err(ProtoError::InvalidQr("duplicate parameter"));
            }
        }
        match v.as_deref() {
            Some("1") => {}
            Some(_) => return Err(ProtoError::InvalidQr("unsupported version")),
            None => return Err(ProtoError::InvalidQr("missing v")),
        }
        let node_id = NodeId::from_base32(&n.ok_or(ProtoError::InvalidQr("missing n"))?)
            .map_err(|_| ProtoError::InvalidQr("bad node id"))?;
        let sec = sec.ok_or(ProtoError::InvalidQr("missing s"))?;
        let raw = Zeroizing::new(
            URL_SAFE_NO_PAD
                .decode(sec.as_bytes())
                .map_err(|_| ProtoError::InvalidQr("bad secret"))?,
        );
        let secret = <[u8; 16]>::try_from(raw.as_slice())
            .map_err(|_| ProtoError::InvalidQr("secret must be 16 bytes"))?;
        Ok(Self {
            node_id,
            secret,
            relay,
        })
    }
}
