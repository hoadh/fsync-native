mod history_common;
use fsync_core::{
    settings::{ClipMode, Settings},
    trust::{PinnedState, SqliteTrustStore, TrustStore},
};
use fsync_proto::{DeviceList, Platform, sign};
use history_common::*;
use uuid::Uuid;

#[tokio::test]
async fn settings_default_roundtrip_and_validation() {
    let tmp = tempfile::tempdir().unwrap();
    let e = open(tmp.path(), &KEY);
    let d = e.hist.settings().await.unwrap();
    assert_eq!(d, Settings::default());
    assert_eq!((d.retention_days, d.retention_bytes), (30, 1 << 30));
    assert_eq!(d.max_clip_bytes, 50 << 20);

    let s = Settings {
        clip_mode: ClipMode::Manual,
        retention_days: 7,
        retention_bytes: 5 << 20,
        max_clip_bytes: 1 << 20,
        denylist: vec!["com.1password.1password".into(), "KeePassXC.exe".into()],
        auto_accept_files: true,
    };
    e.hist.set_settings(s.clone()).await.unwrap();
    drop(e);
    let e = open(tmp.path(), &KEY);
    assert_eq!(e.hist.settings().await.unwrap(), s, "survives restart");

    for bad in [
        Settings {
            retention_days: 0,
            ..s.clone()
        },
        Settings {
            retention_bytes: 0,
            ..s.clone()
        },
        Settings {
            max_clip_bytes: 0,
            ..s.clone()
        },
        Settings {
            denylist: vec![String::new()],
            ..s.clone()
        },
        Settings {
            denylist: vec!["x".repeat(300)],
            ..s.clone()
        },
    ] {
        assert!(e.hist.set_settings(bad).await.is_err());
    }
    assert_eq!(
        e.hist.settings().await.unwrap(),
        s,
        "rejected writes change nothing"
    );
}

fn pinned() -> PinnedState {
    let id = fsync_core::Identity::generate().unwrap();
    let list = DeviceList::genesis(Uuid::new_v4(), id.node_id(), "d".into(), Platform::Cli, 1);
    PinnedState {
        head: sign(id.signing_key(), &list).unwrap(),
        root_hash: [3; 32],
    }
}

#[tokio::test]
async fn sqlite_trust_store_roundtrip_replace_clear() {
    let tmp = tempfile::tempdir().unwrap();
    let e = open(tmp.path(), &KEY);
    let ts = SqliteTrustStore::new(&e.hist);
    assert!(ts.load().unwrap().is_none());
    let a = pinned();
    ts.save(&a).unwrap();
    assert_eq!(ts.load().unwrap(), Some(a.clone()));
    let b = pinned();
    ts.save(&b).unwrap();
    assert_eq!(ts.load().unwrap(), Some(b.clone()), "single row, replaced");
    drop((ts, e));
    let e = open(tmp.path(), &KEY);
    let ts = SqliteTrustStore::new(&e.hist);
    assert_eq!(ts.load().unwrap(), Some(b), "persisted");
    ts.clear().unwrap();
    assert!(ts.load().unwrap().is_none());
}
