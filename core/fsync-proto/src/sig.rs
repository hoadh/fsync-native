//! Domain-separated ed25519 signing helpers.

use crate::{error::ProtoError, fixed_bytes::Sig, node_id::NodeId};
use ed25519_dalek::{Signature, Signer, SigningKey};

fn message(domain: &[u8], body: &[u8]) -> Vec<u8> {
    let mut m = Vec::with_capacity(domain.len() + body.len());
    m.extend_from_slice(domain);
    m.extend_from_slice(body);
    m
}

pub fn sign_domain(key: &SigningKey, domain: &[u8], body: &[u8]) -> Sig {
    Sig::from(key.sign(&message(domain, body)).to_bytes())
}

/// Strict verification (rejects malleable / small-order forms).
pub fn verify_domain(
    signer: &NodeId,
    domain: &[u8],
    body: &[u8],
    sig: &Sig,
) -> Result<(), ProtoError> {
    let vk = signer.verifying_key()?;
    let sig = Signature::from_bytes(&sig.0);
    vk.verify_strict(&message(domain, body), &sig)
        .map_err(|_| ProtoError::BadSignature)
}
