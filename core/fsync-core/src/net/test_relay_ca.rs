//! Test-only hook: trust an extra CA for relay TLS (HTTPS relay + QUIC address discovery).
//!
//! Compiled only with the `test-relay-ca` cargo feature, which no shipped build enables
//! (not default, not fsync-ffi). It lets e2e / netns runs use a relay with a throwaway private CA.
//! The CA is added next to the webpki roots; certificate verification is never switched off.

use crate::error::{CoreError, Result};
use iroh::tls::CaTlsConfig;
use rustls::pki_types::{CertificateDer, pem::PemObject};

/// Env var holding the path of a PEM file with the extra CA certificate(s).
pub const ENV_RELAY_CA: &str = "FSYNC_TEST_RELAY_CA";

/// Webpki roots plus every certificate in `pem`. Errors on empty or malformed PEM.
pub fn ca_config_from_pem(pem: &[u8]) -> Result<CaTlsConfig> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(pem)
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| CoreError::Invalid("invalid relay CA pem"))?;
    if certs.is_empty() {
        return Err(CoreError::Invalid("relay CA pem has no certificate"));
    }
    Ok(CaTlsConfig::embedded().with_extra_roots(certs))
}

/// The CA config from `FSYNC_TEST_RELAY_CA=<path>`; `None` when unset.
pub fn ca_config_from_env() -> Result<Option<CaTlsConfig>> {
    let Some(path) = std::env::var_os(ENV_RELAY_CA) else {
        return Ok(None);
    };
    let pem = std::fs::read(&path)
        .map_err(|_| CoreError::Invalid("cannot read FSYNC_TEST_RELAY_CA file"))?;
    ca_config_from_pem(&pem).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_garbage_pem() {
        assert!(ca_config_from_pem(b"").is_err());
        assert!(
            ca_config_from_pem(b"-----BEGIN CERTIFICATE-----\n!!!\n-----END CERTIFICATE-----\n")
                .is_err()
        );
    }
}
