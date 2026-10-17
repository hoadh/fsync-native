//! `send_files` through the real service and engine: a peer that removed this device answers the
//! clip offer with 403, and the service must say `PeerRefused` (not `NotPaired`, not `PeerOffline`).

use super::*;
use crate::{
    Engine, EngineConfig, EventSink, Identity,
    blobs::FsBlobStore,
    net::Node,
    trust::{MemoryTrustStore, TrustManager},
};
use fsync_proto::{DeviceList, Platform, sign};
use std::{sync::Arc, time::Duration};
use zeroize::Zeroizing;

struct Nop;
impl ServiceDelegate for Nop {
    fn on_remote_clip(&self, _: ClipItem) {}
    fn on_device_status(&self, _: Device) {}
    fn on_transfer(&self, _: TransferEvent) {}
    fn on_incoming_files(&self, _: ClipItem) {}
    fn on_pairing_event(&self, _: PairingEvent) {}
    fn on_account_event(&self, _: AccountEvent) {}
    fn on_session_changed(&self, _: SessionTokens) {}
    fn on_error(&self, _: FsyncError) {}
}

fn service(dir: &std::path::Path) -> FsyncService {
    let cfg = ServiceConfig {
        data_dir: dir.to_owned(),
        api_base: "http://127.0.0.1:9".into(),
        relay_urls: vec![],
        device_name: "sender".into(),
        platform: Platform::Mac,
        device_secret: Zeroizing::new([9; 32]),
        history_key: Zeroizing::new([7; 32]),
        force_relay: false,
        allow_insecure_http: true,
        mdns: false,
    };
    FsyncService::new(cfg, Arc::new(Nop)).unwrap()
}

#[test]
fn send_files_to_a_peer_that_removed_us_is_peer_refused() {
    let (dir, peer_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let svc = service(dir.path());
    svc.start().unwrap();
    let me: fsync_proto::NodeId = svc.node_id().parse().unwrap();

    // The peer is the account root: it lists itself and this device.
    let peer_id = Identity::generate().unwrap();
    let peer_nid = peer_id.node_id();
    let account = uuid::Uuid::new_v4();
    let genesis = sign(
        peer_id.signing_key(),
        &DeviceList::genesis(account, peer_nid, "peer".into(), Platform::Cli, 1),
    )
    .unwrap();
    let head = sign(
        peer_id.signing_key(),
        &DeviceList::next_add(&genesis, peer_nid, me, "sender".into(), Platform::Mac, 2).unwrap(),
    )
    .unwrap();
    svc.inner
        .trust
        .pin_paired_head(&head, &me, &peer_nid, Some(account))
        .unwrap();

    let (sink, _events) = EventSink::channel();
    let peer_trust =
        Arc::new(TrustManager::new(Arc::new(MemoryTrustStore::default()), sink.clone()).unwrap());
    peer_trust.pin_genesis(&genesis).unwrap();
    peer_trust.ingest(&head).unwrap();
    let store = Arc::new(FsBlobStore::open(peer_dir.path().join("blobs")).unwrap());
    let peer = svc
        .run(async {
            let b = Node::builder(peer_id.clone(), peer_trust.clone(), sink.clone());
            Ok(Arc::new(
                Engine::start(b, store, EngineConfig::default())
                    .await
                    .map_err(FsyncError::from)?,
            ))
        })
        .unwrap();
    let port = peer
        .node()
        .endpoint()
        .bound_sockets()
        .iter()
        .find(|s| s.is_ipv4())
        .map(|s| s.port())
        .unwrap();
    svc.inner
        .engine()
        .unwrap()
        .node()
        .discovery
        .hint(&peer_nid, None, &[([127, 0, 0, 1], port).into()])
        .unwrap();

    let file = dir.path().join("f.txt");
    std::fs::write(&file, b"hello").unwrap();
    let path = file.to_string_lossy().into_owned();

    // The peer removes this device; its (and our) view of the account is otherwise unchanged.
    let rm = DeviceList::next_remove(&head, peer_nid, &me, 3).unwrap();
    peer_trust
        .ingest(&sign(peer_id.signing_key(), &rm).unwrap())
        .unwrap();

    let started = std::time::Instant::now();
    let r = svc.send_files(&peer_nid.to_string(), vec![path]);
    assert_eq!(r, Err(FsyncError::PeerRefused), "{r:?}");
    assert!(started.elapsed() < Duration::from_secs(20));
    // We still list the peer as trusted: this is not the local "no such device" case.
    assert!(svc.inner.trust.is_trusted(&peer_nid));
}
