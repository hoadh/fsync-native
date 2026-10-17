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

    let trusting = builder(&url)
        .extra_relay_ca_pem(ca_pem.into_bytes())
        .relay_only(true)
        .spawn()
        .await
        .unwrap();
    assert!(
        trusting.wait_online(Duration::from_secs(15)).await,
        "client with the private CA must reach the relay"
    );

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
