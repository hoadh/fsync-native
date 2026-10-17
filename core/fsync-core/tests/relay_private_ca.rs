//! Feature `test-relay-ca`: a client trusts a relay whose cert is signed by a private CA only when
//! that CA is passed in. Real in-process HTTPS relay on loopback, throwaway CA generated here.
#![cfg(feature = "test-relay-ca")]

use fsync_core::{
    EventSink, Identity,
    net::{Node, NodeBuilder},
    trust::{MemoryTrustStore, TrustManager},
};
use iroh_relay::server::{CertConfig, RelayConfig, Server, ServerConfig, TlsConfig};
use rcgen::{BasicConstraints, CertificateParams, IsCa, KeyPair, SanType};
use std::{net::Ipv4Addr, sync::Arc, time::Duration};

/// (CA pem, relay server rustls config) with a leaf for 127.0.0.1 signed by the CA.
fn private_ca_and_server_config() -> (String, rustls::ServerConfig) {
    let ca_key = KeyPair::generate().unwrap();
    let mut ca_params = CertificateParams::new(vec![]).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca = ca_params.self_signed(&ca_key).unwrap();

    let leaf_key = KeyPair::generate().unwrap();
    let mut leaf_params = CertificateParams::new(vec![]).unwrap();
    leaf_params.subject_alt_names = vec![SanType::IpAddress(Ipv4Addr::LOCALHOST.into())];
    let leaf = leaf_params
        .signed_by(&leaf_key, &rcgen::Issuer::new(ca_params, ca_key))
        .unwrap();

    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(leaf_key.serialize_der().into());
    let cfg = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![leaf.der().clone()], key)
    .unwrap();
    (ca.pem(), cfg)
}

async fn https_relay(server_config: rustls::ServerConfig) -> (Server, String) {
    let mut relay = RelayConfig::new((Ipv4Addr::LOCALHOST, 0));
    relay.tls = Some(TlsConfig::new(
        (Ipv4Addr::LOCALHOST, 0),
        CertConfig::Manual { server_config },
    ));
    let mut cfg = ServerConfig::default();
    cfg.relay = Some(relay);
    let server = Server::spawn(cfg).await.expect("relay starts");
    let url = format!("https://{}", server.https_addr().expect("https addr"));
    (server, url)
}

fn builder(relay: &str) -> NodeBuilder {
    let id = Identity::generate().unwrap();
    let (sink, _events) = EventSink::channel();
    let trust =
        Arc::new(TrustManager::new(Arc::new(MemoryTrustStore::default()), sink.clone()).unwrap());
    Node::builder(id, trust, sink).relays(vec![relay.to_string()])
}

#[tokio::test(flavor = "multi_thread")]
async fn private_relay_ca_is_trusted_only_when_passed_in() {
    let (ca_pem, server_config) = private_ca_and_server_config();
    let (_server, url) = https_relay(server_config).await;

    let ca_pem_bytes = ca_pem.into_bytes();
    let trusting = builder(&url)
        .extra_relay_ca_pem(ca_pem_bytes.clone())
        .relay_only(true)
        .spawn()
        .await
        .unwrap();
    assert!(
        trusting.wait_online(Duration::from_secs(15)).await,
        "client with the private CA must reach the relay"
    );

    // Verification-level control: the same relay endpoint, handshaken with the exact TLS config
    // each client uses. Default roots must fail on the issuer; the hook's roots must succeed.
    let addr = _server.https_addr().unwrap();
    let ca_cfg =
        fsync_core::net::test_relay_ca::ca_config_from_pem(ca_pem_bytes.as_slice()).unwrap();
    let err = handshake(addr, iroh::tls::CaTlsConfig::embedded())
        .await
        .expect_err("default roots must reject a private CA");
    assert!(
        err.contains("UnknownIssuer"),
        "expected a verification failure, got: {err}"
    );
    handshake(addr, ca_cfg)
        .await
        .expect("hook roots must verify the relay cert");

    let plain = builder(&url).relay_only(true).spawn().await.unwrap();
    assert!(
        !plain.wait_online(Duration::from_secs(4)).await,
        "client without the CA must not trust the relay cert"
    );
}

#[tokio::test]
async fn malformed_ca_pem_is_rejected_at_spawn() {
    let res = builder("https://127.0.0.1:1")
        .extra_relay_ca_pem(b"not a pem".to_vec())
        .spawn()
        .await;
    assert!(res.is_err());
}

/// TLS handshake to the relay with `ca`; the error text names the verification failure.
async fn handshake(addr: std::net::SocketAddr, ca: iroh::tls::CaTlsConfig) -> Result<(), String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let cfg = ca.client_config(provider).map_err(|e| e.to_string())?;
    let tcp = tokio::net::TcpStream::connect(addr)
        .await
        .map_err(|e| e.to_string())?;
    let name = rustls::pki_types::ServerName::IpAddress(addr.ip().into());
    tokio_rustls::TlsConnector::from(Arc::new(cfg))
        .connect(name, tcp)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}
