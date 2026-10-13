//! JSON bodies for the backend HTTP API (`/v1/...`).

use crate::{device_list::SignedDeviceList, error::ProtoError, node_id::NodeId};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// POST /v1/auth/otp/start -> 204
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtpStart {
    pub email: String,
}

/// POST /v1/auth/otp/verify
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtpVerify {
    pub email: String,
    pub code: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenPair {
    pub access: String,
    pub refresh: String,
}

// Tokens are secrets: never print them.
impl std::fmt::Debug for TokenPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TokenPair(<redacted>)")
    }
}

/// Response of POST /v1/auth/otp/verify: `{access, refresh, account_id}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtpVerifyResponse {
    #[serde(flatten)]
    pub tokens: TokenPair,
    pub account_id: Uuid,
}

/// POST /v1/auth/refresh (-> TokenPair) and POST /v1/auth/logout (-> 204)
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refresh {
    pub refresh: String,
}

impl std::fmt::Debug for Refresh {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Refresh(<redacted>)")
    }
}

/// GET /v1/device-list response. `signed` = base64(postcard(SignedDeviceList)).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceListResponse {
    pub signed: String,
    pub seq: u64,
}

/// GET /v1/device-list/chain: consecutive lists (base64 each), ascending seq, at most 100.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainResponse {
    pub chain: Vec<String>,
    /// Lowest seq the server still retains for the current epoch. A request with
    /// `from_seq < earliest_seq` cannot be served contiguously (pruned history).
    #[serde(default)]
    pub earliest_seq: u64,
    /// More lists with `seq >= from_seq` exist beyond `chain`: ask again from the last seq + 1.
    #[serde(default)]
    pub truncated: bool,
}

/// PUT /v1/device-list (header `If-Match: <seq>`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PutDeviceList {
    pub signed: String,
}

pub fn encode_signed(signed: &SignedDeviceList) -> Result<String, ProtoError> {
    Ok(STANDARD.encode(signed.to_bytes()?))
}

pub fn decode_signed(b64: &str) -> Result<SignedDeviceList, ProtoError> {
    let raw = STANDARD
        .decode(b64)
        .map_err(|_| ProtoError::InvalidBase64)?;
    SignedDeviceList::from_bytes(&raw)
}

impl DeviceListResponse {
    pub fn new(signed: &SignedDeviceList) -> Result<Self, ProtoError> {
        Ok(Self {
            signed: encode_signed(signed)?,
            seq: signed.decode()?.seq,
        })
    }
    pub fn decode(&self) -> Result<SignedDeviceList, ProtoError> {
        decode_signed(&self.signed)
    }
}

impl PutDeviceList {
    pub fn new(signed: &SignedDeviceList) -> Result<Self, ProtoError> {
        Ok(Self {
            signed: encode_signed(signed)?,
        })
    }
    pub fn decode(&self) -> Result<SignedDeviceList, ProtoError> {
        decode_signed(&self.signed)
    }
}

/// POST /v1/pairing
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingCreate {
    pub node_id: NodeId,
    pub relay_url: Option<String>,
}

/// Response of POST /v1/pairing. `expires_at` = unix epoch milliseconds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingCreated {
    pub code: String,
    pub expires_at: u64,
}

/// Response of GET /v1/pairing/{code}.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingLookup {
    pub node_id: NodeId,
    pub relay_url: Option<String>,
}

/// Pairing codes are exactly 6 ASCII digits.
pub fn is_valid_pairing_code(code: &str) -> bool {
    code.len() == 6 && code.bytes().all(|b| b.is_ascii_digit())
}

/// Error body for non-2xx responses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
}
