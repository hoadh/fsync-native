//! `history_item_reps`: the full content of a history item, not just its preview (real backend).

mod backend_harness;
mod common;
use backend_harness::Backend;
use fsync_core::service::*;
use fsync_proto::Platform;
use std::sync::Arc;
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

fn service(dir: &std::path::Path, api: &str) -> FsyncService {
    let cfg = ServiceConfig {
        data_dir: dir.to_owned(),
        api_base: api.into(),
        relay_urls: vec![],
        device_name: "reps-test".into(),
        platform: Platform::Mac,
        device_secret: Zeroizing::new([3; 32]),
        history_key: Zeroizing::new([7; 32]),
        force_relay: false,
        allow_insecure_http: true,
        mdns: false,
    };
    FsyncService::new(cfg, Arc::new(Nop)).unwrap()
}

/// A signed-in service with an account, so `publish_clip` works (no peers: nothing is sent).
fn started(rt: &tokio::runtime::Runtime, dir: &std::path::Path) -> (FsyncService, String) {
    let be = rt.block_on(Backend::start());
    let svc = service(dir, &be.base);
    svc.login_start("a@example.com").unwrap();
    svc.login_verify("a@example.com", &be.last_code("a@example.com"))
        .unwrap();
    svc.ensure_account().unwrap();
    svc.start().unwrap();
    (svc, be.base)
}

fn rep(mime: &str, data: &[u8]) -> ClipRep {
    ClipRep {
        mime: mime.into(),
        data: data.to_vec(),
    }
}

#[test]
fn full_reps_come_back_in_order_and_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    // Longer than any preview cut (200 chars), to prove this is not the preview.
    let long = "x".repeat(5_000);
    let png = vec![0x89u8, b'P', b'N', b'G', 0, 1, 2, 3];
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (svc, api) = started(&rt, dir.path());
    let p = svc
        .publish_clip(LocalClip {
            reps: vec![
                rep("text/plain", long.as_bytes()),
                rep("text/html", b"<b>x</b>"),
                rep("image/png", &png),
            ],
            files: vec![],
            preview: None,
            sensitive: false,
        })
        .unwrap();

    let check = |svc: &FsyncService| {
        let reps = svc.history_item_reps(&p.item_id).unwrap();
        let got: Vec<_> = reps.iter().map(|r| r.mime.as_str()).collect();
        assert_eq!(got, ["text/plain", "text/html", "image/png"]);
        assert_eq!(reps[0].data, long.as_bytes());
        assert_eq!(reps[2].data, png);
    };
    check(&svc);
    let preview = svc.history(None, None, 1).unwrap().items[0]
        .preview
        .clone()
        .unwrap();
    assert!(preview.len() < long.len());

    // Stored reps need no running engine and no network.
    svc.stop().unwrap();
    check(&svc);
    drop(svc);
    check(&service(dir.path(), &api));
}

#[test]
fn unknown_and_malformed_ids_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let svc = service(dir.path(), "http://127.0.0.1:9");
    let unknown = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        svc.history_item_reps(&unknown).err(),
        Some(FsyncError::NotFound)
    );
    assert!(matches!(
        svc.history_item_reps("nope"),
        Err(FsyncError::Invalid(_))
    ));
}

#[test]
fn a_deleted_item_has_no_reps() {
    let dir = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (svc, _api) = started(&rt, dir.path());
    let p = svc
        .publish_clip(LocalClip {
            reps: vec![rep("text/plain", b"gone soon")],
            files: vec![],
            preview: None,
            sensitive: false,
        })
        .unwrap();
    assert_eq!(svc.history_item_reps(&p.item_id).unwrap().len(), 1);
    svc.delete(&p.item_id).unwrap();
    assert_eq!(
        svc.history_item_reps(&p.item_id).err(),
        Some(FsyncError::NotFound)
    );
}

#[test]
fn sensitive_clips_leave_nothing_to_restore() {
    let dir = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (svc, _api) = started(&rt, dir.path());
    let p = svc
        .publish_clip(LocalClip {
            reps: vec![rep("text/plain", b"hunter2")],
            files: vec![],
            preview: None,
            sensitive: true,
        })
        .unwrap();
    assert_eq!(
        svc.history_item_reps(&p.item_id).err(),
        Some(FsyncError::NotFound)
    );
}
