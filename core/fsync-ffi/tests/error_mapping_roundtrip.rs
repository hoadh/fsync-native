//! FFI error contract v1: every service error maps to its FFI twin and back without loss,
//! and the FFI message text stays identical to the service text (Swift shows it to users).

use fsync_core::service::FsyncError as Svc;
use fsync_ffi::FsyncError as Ffi;

/// One sample per variant. The exhaustive match below fails to compile when a variant is added
/// without a sample here, and the length check fails when a sample is missing.
fn all_variants() -> Vec<Svc> {
    let v = vec![
        Svc::NotPaired,
        Svc::PeerOffline,
        Svc::Network("dns timeout".into()),
        Svc::Integrity,
        Svc::TooLarge,
        Svc::Auth,
        Svc::ReauthRequired,
        Svc::Io("disk full".into()),
        Svc::Cancelled,
        Svc::HistoryKeyMismatch,
        Svc::NotFound,
        Svc::Invalid("bad peer id".into()),
        Svc::EngineAlreadyRunning,
        Svc::PeerRefused,
        Svc::Internal("unreachable state".into()),
    ];
    let distinct: std::collections::HashSet<_> = v.iter().map(variant_index).collect();
    assert_eq!(distinct.len(), v.len(), "duplicate sample");
    assert_eq!(distinct.len(), VARIANT_COUNT, "a variant has no sample");
    v
}

const VARIANT_COUNT: usize = 15;

/// Exhaustive on purpose: adding a variant breaks the build here.
fn variant_index(e: &Svc) -> usize {
    match e {
        Svc::NotPaired => 0,
        Svc::PeerOffline => 1,
        Svc::Network(_) => 2,
        Svc::Integrity => 3,
        Svc::TooLarge => 4,
        Svc::Auth => 5,
        Svc::Io(_) => 6,
        Svc::Cancelled => 7,
        Svc::HistoryKeyMismatch => 8,
        Svc::NotFound => 9,
        Svc::Invalid(_) => 10,
        Svc::Internal(_) => 11,
        Svc::ReauthRequired => 12,
        Svc::EngineAlreadyRunning => 13,
        Svc::PeerRefused => 14,
    }
}

#[test]
fn every_variant_roundtrips_through_ffi() {
    for e in all_variants() {
        let back: Svc = Ffi::from(e.clone()).into();
        assert_eq!(back, e);
    }
}

#[test]
fn ffi_messages_match_service_messages() {
    for e in all_variants() {
        assert_eq!(Ffi::from(e.clone()).to_string(), e.to_string());
    }
}

/// A peer that removed us is its own error, not "not paired"; `Published` keeps `refused` apart
/// from `failed`.
#[test]
fn peer_refusal_is_distinct_and_published_keeps_refused_apart() {
    use fsync_core::{error::CoreError, service::Published as SvcPublished};
    assert_eq!(Svc::from(CoreError::PeerRefused), Svc::PeerRefused);
    assert_eq!(Svc::from(CoreError::NotTrusted("x".into())), Svc::NotPaired);
    assert_ne!(
        Ffi::from(Svc::PeerRefused).to_string(),
        Ffi::from(Svc::NotPaired).to_string()
    );
    let p: fsync_ffi::Published = SvcPublished {
        item_id: "i".into(),
        echo: false,
        delivered: vec!["a".into()],
        failed: vec!["b".into()],
        refused: vec!["c".into()],
    }
    .into();
    assert_eq!((p.failed, p.refused), (vec!["b".into()], vec!["c".into()]));
}
