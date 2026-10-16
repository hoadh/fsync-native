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

#[test]
fn device_with_nothing_pinned_still_reports_the_unreachable_backend() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let be = rt.block_on(Backend::start());
    let d = dev_with(&be, 2, "dev-b", false);
    d.login(&be);
    drop(rt);

    assert!(d.svc.ensure_account().is_err());
}
