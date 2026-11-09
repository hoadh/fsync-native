//! Secret material the app persists outside the engine. Zeroized on drop, never logged.

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Fixed-size keys as hex strings in JSON (readable, no number arrays).
mod hex32 {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn serialize<S: Serializer>(v: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        let mut out = String::with_capacity(64);
        for b in v {
            out.push_str(&format!("{b:02x}"));
        }
        s.serialize_str(&out)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(d)?;
        if s.len() != 64 || !s.is_ascii() {
            return Err(D::Error::custom("expected 64 hex chars"));
        }
        let mut out = [0u8; 32];
        for (i, b) in out.iter_mut().enumerate() {
            *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(D::Error::custom)?;
        }
        Ok(out)
    }
}

#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct SessionTokens {
    pub access: String,
    pub refresh: String,
    pub account_id: String,
}

#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Secrets {
    #[serde(with = "hex32")]
    pub device_secret: [u8; 32],
    #[serde(with = "hex32")]
    pub history_key: [u8; 32],
    #[serde(default)]
    pub session: Option<SessionTokens>,
    #[serde(default)]
    pub email: Option<String>,
    /// Refresh token of a sign-out whose server-side revoke has not succeeded yet.
    #[serde(default)]
    pub pending_revoke: Option<String>,
}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secrets(<redacted>)")
    }
}

impl std::fmt::Debug for SessionTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionTokens(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn sample() -> Secrets {
        Secrets {
            device_secret: [1; 32],
            history_key: [2; 32],
            session: Some(SessionTokens {
                access: "a".into(),
                refresh: "r".into(),
                account_id: "id".into(),
            }),
            email: Some("a@b.c".into()),
            pending_revoke: None,
        }
    }

    #[test]
    fn json_roundtrip_and_redacted_debug() {
        let s = sample();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains(&"01".repeat(32)));
        let back: Secrets = serde_json::from_str(&json).unwrap();
        assert_eq!(back.history_key, [2; 32]);
        assert_eq!(back.session.as_ref().unwrap().refresh, "r");
        assert!(!format!("{s:?}").contains("a@b.c"));
    }

    #[test]
    fn bad_hex_rejected() {
        let bad = r#"{"device_secret":"zz","history_key":"00"}"#;
        assert!(serde_json::from_str::<Secrets>(bad).is_err());
    }
}
