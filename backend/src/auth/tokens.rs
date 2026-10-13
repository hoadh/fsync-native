//! Minimal EdDSA (ed25519) JWT: fixed header, strict verification, no algorithm negotiation.

use crate::{
    error::{ApiError, ApiResult},
    util::now_ms,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const ACCESS_TTL_S: i64 = 15 * 60;
const HEADER_JSON: &str = r#"{"alg":"EdDSA","typ":"JWT"}"#;

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    /// Refresh family (session) id.
    sid: Uuid,
    /// Unix s of the OTP login that started the session (not renewed by refresh).
    auth_time: i64,
    iat: i64,
    exp: i64,
}

/// Verified access-token contents.
#[derive(Clone, Copy, Debug)]
pub struct AccessClaims {
    pub account: Uuid,
    pub sid: Uuid,
    pub auth_time: i64,
    pub exp: i64,
}

pub struct TokenService {
    signing: SigningKey,
    verifying: VerifyingKey,
}

impl TokenService {
    pub fn new(seed: &[u8; 32]) -> Self {
        let signing = SigningKey::from_bytes(seed);
        let verifying = signing.verifying_key();
        Self { signing, verifying }
    }

    /// `auth_time_ms`: OTP login time of the session, as stored on the refresh family.
    pub fn issue_access(&self, account: Uuid, sid: Uuid, auth_time_ms: i64) -> ApiResult<String> {
        let iat = now_ms() / 1000;
        self.issue_with(account, sid, auth_time_ms / 1000, iat, iat + ACCESS_TTL_S)
    }

    /// Explicit timestamps (unix s); public so tests can mint stale or short-lived tokens.
    pub fn issue_with(
        &self,
        account: Uuid,
        sid: Uuid,
        auth_time: i64,
        iat: i64,
        exp: i64,
    ) -> ApiResult<String> {
        let claims = serde_json::to_vec(&Claims {
            sub: account,
            sid,
            auth_time,
            iat,
            exp,
        })
        .map_err(ApiError::internal)?;
        let signing_input = format!("{}.{}", B64.encode(HEADER_JSON), B64.encode(claims));
        let sig = self.signing.sign(signing_input.as_bytes());
        Ok(format!("{signing_input}.{}", B64.encode(sig.to_bytes())))
    }

    /// Returns the claims of a valid, unexpired token.
    pub fn verify_access(&self, token: &str) -> ApiResult<AccessClaims> {
        let mut parts = token.split('.');
        let (Some(h), Some(p), Some(s), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(ApiError::Unauthorized);
        };
        if B64.decode(h).map_err(|_| ApiError::Unauthorized)? != HEADER_JSON.as_bytes() {
            return Err(ApiError::Unauthorized);
        }
        let sig_bytes = B64.decode(s).map_err(|_| ApiError::Unauthorized)?;
        let sig = Signature::from_slice(&sig_bytes).map_err(|_| ApiError::Unauthorized)?;
        let signing_input = &token[..h.len() + 1 + p.len()];
        self.verifying
            .verify_strict(signing_input.as_bytes(), &sig)
            .map_err(|_| ApiError::Unauthorized)?;
        let claims: Claims =
            serde_json::from_slice(&B64.decode(p).map_err(|_| ApiError::Unauthorized)?)
                .map_err(|_| ApiError::Unauthorized)?;
        if claims.exp <= now_ms() / 1000 {
            return Err(ApiError::Unauthorized);
        }
        Ok(AccessClaims {
            account: claims.sub,
            sid: claims.sid,
            auth_time: claims.auth_time,
            exp: claims.exp,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_rejections() {
        let svc = TokenService::new(&[7; 32]);
        let (acct, sid) = (Uuid::new_v4(), Uuid::new_v4());
        let tok = svc.issue_access(acct, sid, 1_700_000_000_000).unwrap();
        let c = svc.verify_access(&tok).unwrap();
        assert_eq!((c.account, c.sid, c.auth_time), (acct, sid, 1_700_000_000));

        // other key
        assert!(TokenService::new(&[8; 32]).verify_access(&tok).is_err());
        // tampered payload
        let mut parts: Vec<&str> = tok.split('.').collect();
        let forged = B64.encode(format!(
            r#"{{"sub":"{}","sid":"{sid}","auth_time":0,"iat":0,"exp":99999999999}}"#,
            Uuid::new_v4()
        ));
        parts[1] = &forged;
        assert!(svc.verify_access(&parts.join(".")).is_err());
        // expired
        assert!(
            svc.verify_access(&svc.issue_with(acct, sid, 0, 0, 1).unwrap())
                .is_err()
        );
        // alg=none style header
        let none = format!("{}.{}.", B64.encode(r#"{"alg":"none"}"#), B64.encode("{}"));
        assert!(svc.verify_access(&none).is_err());
        assert!(svc.verify_access("garbage").is_err());
    }
}
