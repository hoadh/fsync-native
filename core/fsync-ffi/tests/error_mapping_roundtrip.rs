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
        Svc::Internal("unreachable state".into()),
    ];
    let distinct: std::collections::HashSet<_> = v.iter().map(variant_index).collect();
    assert_eq!(distinct.len(), v.len(), "duplicate sample");
    assert_eq!(distinct.len(), VARIANT_COUNT, "a variant has no sample");
    v
}

const VARIANT_COUNT: usize = 13;

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
