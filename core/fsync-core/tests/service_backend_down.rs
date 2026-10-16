//! A device that already pinned its device list starts and stays usable when the backend is
//! unreachable (design N2 "WAN down", N9 "backend down"); one with nothing pinned still errors.

mod backend_harness;
mod common;
mod service_harness;
use backend_harness::Backend;
use fsync_core::service::*;
use service_harness::*;

#[test]
fn pinned_device_is_ready_from_its_cached_list_when_the_backend_is_gone() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let be = rt.block_on(Backend::start());
    let a = start_account_with(&be, 1, "dev-a", false);
    // Dropping the runtime stops the backend: its port now refuses connections.
    drop(rt);

    assert_eq!(a.svc.ensure_account().unwrap(), AccountState::Ready);
    let me = a.svc.devices();
    assert!(me.len() == 1 && me[0].is_self);
}

/// Offline start is bounded: a list last confirmed more than 7 days ago is refused.
#[test]
fn cached_list_older_than_the_bound_is_refused_offline() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let be = rt.block_on(Backend::start());
    let a = start_account_with(&be, 1, "dev-a", false);
    let stamp = a.data_dir().join("list-synced-at");
    assert!(stamp.exists(), "a successful sync records its time");
    drop(rt);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    std::fs::write(&stamp, (now - 8 * 24 * 3600).to_string()).unwrap();
    assert!(matches!(
        a.svc.ensure_account(),
        Err(FsyncError::Network(_))
    ));

    std::fs::write(&stamp, (now - 6 * 24 * 3600).to_string()).unwrap();
    assert_eq!(a.svc.ensure_account().unwrap(), AccountState::Ready);
}

#[test]
fn device_with_nothing_pinned_still_reports_the_unreachable_backend() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let be = rt.block_on(Backend::start());
    let d = dev_with(&be, 2, "dev-b", false);
    d.login(&be);
    drop(rt);

    assert!(d.svc.ensure_account().is_err());
}

fn stamp_secs(path: &std::path::Path) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// A long-running app keeps its offline window open through the presence path alone: a list
/// change pushed by the backend makes it re-sync, and that refreshes the stamp.
#[test]
fn presence_resync_refreshes_the_stamp_so_a_long_running_app_may_start_offline() {
    let _lan = lan_serial();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let be = rt.block_on(Backend::start());
    let a = start_account(&be, 1, "dev-a");
    let b = dev_with(&be, 2, "dev-b", true);
    b.login(&be);
    assert_eq!(b.svc.ensure_account().unwrap(), AccountState::NeedsPairing);
    pair(&a, &b);
    let c = dev_with(&be, 3, "dev-c", true);
    c.login(&be);
    assert_eq!(c.svc.ensure_account().unwrap(), AccountState::NeedsPairing);

    // B has been up for a long time: its last confirmation is 8 days old.
    let stamp = b.data_dir().join("list-synced-at");
    let old = stamp_secs(&stamp).unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }) - 8 * 24 * 3600;
    std::fs::write(&stamp, old.to_string()).unwrap();

    // A pairs C: the backend pushes DeviceListChanged to B, whose presence pump re-syncs.
    pair(&a, &c);
    wait_for("B's presence resync to refresh the stamp", || {
        stamp_secs(&stamp).is_some_and(|t| t > old + 7 * 24 * 3600)
    });

    drop(rt);
    assert_eq!(b.svc.ensure_account().unwrap(), AccountState::Ready);
}
