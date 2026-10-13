//! pair/1 key schedule: shared key -> MAC key + SAS words. All secrets are zeroized.

use fsync_proto::{NodeId, pairing::PairMode};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

const LABEL: &[u8] = b"fsync-pair-v1";

/// Which side is computing a confirmation MAC (prevents reflecting a MAC back).
#[derive(Clone, Copy)]
pub enum Role {
    Existing,
    New,
}

impl Role {
    fn tag(self) -> &'static [u8] {
        match self {
            Role::Existing => b"confirm-existing",
            Role::New => b"confirm-new",
        }
    }
}

/// QR mode shared key: HKDF(secret128, salt = node_E || node_N).
pub fn qr_key(secret: &[u8; 16], existing: &NodeId, new: &NodeId) -> Zeroizing<[u8; 32]> {
    let mut salt = Vec::with_capacity(64);
    salt.extend_from_slice(existing.as_bytes());
    salt.extend_from_slice(new.as_bytes());
    let mut out = Zeroizing::new([0u8; 32]);
    // 32 bytes is far below the HKDF-SHA256 output limit: expand cannot fail.
    let _ = Hkdf::<Sha256>::new(Some(&salt), secret).expand(b"fsync-pair-v1 qr key", out.as_mut());
    out
}

/// Hash binding mode, both ids, both per-session nonces and the SPAKE2 messages (empty in QR
/// mode). The nonces make QR-mode keys, MACs and SAS fresh for every session.
pub fn transcript(
    mode: PairMode,
    existing: &NodeId,
    new: &NodeId,
    nonces: (&[u8; 16], &[u8; 16]),
    msg_existing: &[u8],
    msg_new: &[u8],
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(LABEL);
    h.update([match mode {
        PairMode::Code => 0u8,
        PairMode::Qr => 1u8,
    }]);
    h.update(existing.as_bytes());
    h.update(new.as_bytes());
    h.update(nonces.0);
    h.update(nonces.1);
    for m in [msg_existing, msg_new] {
        h.update((m.len() as u32).to_le_bytes());
        h.update(m);
    }
    h.finalize().into()
}

pub struct PairKeys {
    mac_key: Zeroizing<[u8; 32]>,
    sas: Zeroizing<[u8; 8]>,
}

impl PairKeys {
    pub fn derive(shared: &[u8], transcript: &[u8; 32]) -> Self {
        let hk = Hkdf::<Sha256>::new(Some(transcript), shared);
        let mut mac_key = Zeroizing::new([0u8; 32]);
        let mut sas = Zeroizing::new([0u8; 8]);
        // Output sizes are below the HKDF limit: expand cannot fail.
        let _ = hk.expand(b"fsync-pair-v1 mac", mac_key.as_mut());
        let _ = hk.expand(b"fsync-pair-v1 sas", sas.as_mut());
        Self { mac_key, sas }
    }

    pub fn mac(&self, role: Role, transcript: &[u8; 32]) -> [u8; 32] {
        let mut m = <Hmac<Sha256> as Mac>::new_from_slice(self.mac_key.as_ref())
            .unwrap_or_else(|_| unreachable!("HMAC accepts any key length"));
        m.update(role.tag());
        m.update(transcript);
        m.finalize().into_bytes().into()
    }

    /// Constant-time check of the peer's confirmation MAC.
    pub fn verify(&self, role: Role, transcript: &[u8; 32], got: &[u8; 32]) -> bool {
        self.mac(role, transcript).ct_eq(got).into()
    }

    /// Three pronounceable words (36 bits) for the user to compare on both screens.
    pub fn sas(&self) -> Zeroizing<String> {
        const C: &[u8; 16] = b"bdfghjklmnprstvz";
        const V: &[u8; 4] = b"aeio";
        let n = u64::from_le_bytes(*self.sas);
        let word = |w: u64| -> String {
            let mut s = String::with_capacity(4);
            s.push(C[(w & 15) as usize] as char);
            s.push(V[((w >> 4) & 3) as usize] as char);
            s.push(C[((w >> 6) & 15) as usize] as char);
            s.push(V[((w >> 10) & 3) as usize] as char);
            s
        };
        Zeroizing::new(format!("{} {} {}", word(n), word(n >> 12), word(n >> 24)))
    }
}
