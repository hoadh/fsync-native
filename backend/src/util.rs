//! Small shared helpers: clock, randomness, keyed hashing.

use crate::error::{ApiError, ApiResult};
use std::time::{SystemTime, UNIX_EPOCH};

/// Unix epoch milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

pub fn random_bytes<const N: usize>() -> ApiResult<[u8; N]> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).map_err(ApiError::internal)?;
    Ok(buf)
}

/// Uniform 6-digit decimal string (rejection sampling, no modulo bias).
pub fn random_digits6() -> ApiResult<String> {
    const LIMIT: u32 = 4_294_000_000; // largest multiple of 1_000_000 <= u32::MAX
    loop {
        let n = u32::from_le_bytes(random_bytes::<4>()?);
        if n < LIMIT {
            return Ok(format!("{:06}", n % 1_000_000));
        }
    }
}

/// Keyed blake3 over domain-separated parts. Used for OTP and pairing codes, which have low
/// entropy: the server-side key makes a leaked DB useless for offline guessing.
pub fn keyed_hash(key: &[u8; 32], domain: &str, parts: &[&[u8]]) -> blake3::Hash {
    let mut h = blake3::Hasher::new_keyed(key);
    h.update(domain.as_bytes());
    for p in parts {
        h.update(&u64::try_from(p.len()).unwrap_or(u64::MAX).to_le_bytes());
        h.update(p);
    }
    h.finalize()
}

/// Convert a stored 32-byte column into a hash; `None` if the length is wrong.
pub fn hash_from_slice(b: &[u8]) -> Option<blake3::Hash> {
    <[u8; 32]>::try_from(b).ok().map(blake3::Hash::from_bytes)
}

pub const MAX_RELAY_URL: usize = 512;

/// Relay URLs must be https (plain http only in dev mode), have a host, and stay short.
pub fn relay_url_ok(raw: &str, dev_mode: bool) -> bool {
    raw.len() <= MAX_RELAY_URL
        && url::Url::parse(raw).is_ok_and(|u| {
            u.host_str().is_some() && (u.scheme() == "https" || (dev_mode && u.scheme() == "http"))
        })
}

#[cfg(test)]
mod tests {
    use super::relay_url_ok;

    #[test]
    fn relay_scheme_rules() {
        assert!(relay_url_ok("https://relay.example.com", false));
        assert!(!relay_url_ok("http://relay.example.com", false));
        assert!(relay_url_ok("http://localhost:3340", true));
        assert!(!relay_url_ok("javascript:alert(1)", true));
        assert!(!relay_url_ok("https://", true));
        assert!(!relay_url_ok(
            &format!("https://a.com/{}", "x".repeat(600)),
            true
        ));
    }
}
