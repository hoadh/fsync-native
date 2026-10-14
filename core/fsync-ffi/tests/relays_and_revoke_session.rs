//! FFI additions after contract v2: `EngineConfig.relay_urls` (merged with `relay_url`,
//! validated at construction) and the engine-free `revoke_session`.

use fsync_ffi::*;
use std::sync::Arc;

struct Nop;
impl EngineDelegate for Nop {
    fn on_remote_clip(&self, _: ClipItem) {}
    fn on_device_status(&self, _: Device) {}
    fn on_transfer(&self, _: TransferEvent) {}
    fn on_incoming_files(&self, _: ClipItem) {}
    fn on_pairing_event(&self, _: PairingEvent) {}
    fn on_account_event(&self, _: AccountEvent) {}
    fn on_session_changed(&self, _: SessionTokens) {}
    fn on_error(&self, _: FsyncError) {}
}

fn cfg(tag: &str, relay_url: Option<&str>, relay_urls: &[&str], http: bool) -> EngineConfig {
    let dir = std::env::temp_dir().join(format!("fsync-ffi-relays-{}-{tag}", std::process::id()));
    EngineConfig {
        data_dir: dir.to_string_lossy().into_owned(),
        api_base: "https://api.example.invalid".into(),
        relay_url: relay_url.map(str::to_owned),
        relay_urls: relay_urls.iter().map(|s| s.to_string()).collect(),
        device_name: "ffi-test".into(),
        platform: Platform::Cli,
        device_secret: vec![1; 32],
        history_key: vec![2; 32],
        force_relay: false,
        allow_insecure_http: http,
    }
}

fn open(c: EngineConfig) -> Result<Arc<FsyncEngine>, FsyncError> {
    let dir = c.data_dir.clone();
    std::fs::create_dir_all(&dir).unwrap();
    let r = FsyncEngine::new(c, Arc::new(Nop));
    if r.is_err() {
        let _ = std::fs::remove_dir_all(dir);
    }
    r
}

#[test]
fn relay_lists_are_validated_when_the_engine_is_built() {
    let e = open(cfg(
        "ok",
        Some("https://relay-us.example"),
        &["https://relay-sg.example", "https://relay-us.example/"],
        false,
    ))
    .expect("relay_url + relay_urls, duplicates merged");
    drop(e);
    for (tag, one, many, http) in [
        ("http", Some("http://relay.example"), &[][..], false),
        (
            "http-list",
            None,
            &["https://ok.example", "http://relay.example"][..],
            false,
        ),
        ("junk", None, &["not a url"][..], true),
        ("scheme", Some("ftp://relay.example"), &[][..], true),
    ] {
        let r = open(cfg(tag, one, many, http));
        assert!(
            matches!(r, Err(FsyncError::Invalid { .. })),
            "{tag}: {:?}",
            r.err()
        );
    }
    let dev = open(cfg("dev", None, &["http://127.0.0.1:3340"], true));
    assert!(dev.is_ok(), "plain http relay is allowed in dev mode");
}

#[test]
fn revoke_session_validates_before_any_network() {
    let bad = revoke_session("http://127.0.0.1:1".into(), "tok".into(), false);
    assert!(matches!(bad, Err(FsyncError::Invalid { .. })), "{bad:?}");
    let empty = revoke_session("https://api.example.invalid".into(), String::new(), false);
    assert!(
        matches!(empty, Err(FsyncError::Invalid { .. })),
        "{empty:?}"
    );
    // Nothing listens on port 1: refused, which is a retryable error.
    let refused = revoke_session("http://127.0.0.1:1".into(), "tok".into(), true);
    assert!(
        matches!(refused, Err(FsyncError::Network { .. })),
        "{refused:?}"
    );
}
