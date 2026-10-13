//! One rustls client config (ring provider, webpki roots) for HTTPS and WSS.

use crate::error::{CoreError, Result};
use rustls::{ClientConfig, RootCertStore, crypto::ring};
use std::sync::Arc;

pub fn client_config() -> Result<Arc<ClientConfig>> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let cfg = ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| CoreError::Http(format!("tls config: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(cfg))
}
