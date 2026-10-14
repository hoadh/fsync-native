//! Trust-reset edge cases through `FsyncService` against the real backend: the device still
//! listed after a reset, a grown offer, a pinned initiator, wrong accounts, retries, races.

mod backend_harness;
mod common;
mod service_harness;
use backend_harness::Backend;
use fsync_core::service::*;
use service_harness::*;
use std::time::Duration;

fn backend() -> (tokio::runtime::Runtime, Backend) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let be = rt.block_on(Backend::start());
    (rt, be)
}

/// A device that only logged in: no pin, so it can reset the account.
fn logged_in(be: &Backend, seed: u8, name: &str, mdns: bool) -> Dev {
    let d = dev_with(be, seed, name, mdns);
    d.login(be);
    assert_eq!(d.svc.ensure_account().unwrap(), AccountState::NeedsPairing);
    d
}

#[test]
fn confirming_a_list_that_still_has_this_device_is_ready() {
    let _lan = lan_serial();
    let (_rt, be) = backend();
    let a = start_account(&be, 1, "dev-a");
    let b = logged_in(&be, 2, "dev-b", true);
    pair(&a, &b); // the list has two devices, so a reset really changes it
    // The same device identity after a reinstall: no pin, so it can reset the account.
    let again = logged_in(&be, 1, "dev-a-again", false);
    assert_eq!(
        again.svc.reset_account_trust().unwrap(),
        AccountState::Created
    );

    assert_eq!(a.svc.ensure_account().unwrap(), AccountState::Ready);
    wait_for("TrustReset on A", || !a.rec.resets().is_empty());
    assert_eq!(a.rec.reset_names(), vec![vec!["dev-a-again".to_string()]]);
    let id = a.svc.pending_trust_resets()[0].reset_id.clone();
    assert_eq!(a.svc.confirm_trust_reset(&id).unwrap(), AccountState::Ready);
    let devices = a.svc.devices();
    assert!(devices.len() == 1 && devices[0].is_self);
    assert_eq!(a.rec.count(|e| matches!(e, AccountEvent::SelfRemoved)), 0);
    assert!(a.svc.pending_trust_resets().is_empty());
}

#[test]
fn a_grown_offer_is_announced_again_and_the_shown_id_stops_working() {
    let _lan = lan_serial();
    let (_rt, be) = backend();
    let a = start_account(&be, 1, "dev-a");
    let c = logged_in(&be, 3, "dev-c", true);
    c.svc.reset_account_trust().unwrap();
    assert_eq!(a.svc.ensure_account().unwrap(), AccountState::Ready);
    wait_for("first announcement", || a.rec.resets().len() == 1);
    let shown = a.svc.pending_trust_resets()[0].reset_id.clone();

    // The offered chain grows before A answers.
    let d = logged_in(&be, 4, "dev-d", true);
    pair(&c, &d);
    assert_eq!(a.svc.ensure_account().unwrap(), AccountState::Ready);
    wait_for("second announcement", || a.rec.resets().len() == 2);
    assert_eq!(
        a.rec.reset_names(),
        vec![
            vec!["dev-c".to_string()],
            vec!["dev-c".to_string(), "dev-d".to_string()]
        ]
    );
    let now = a.svc.pending_trust_resets();
    assert_eq!(now.len(), 1);
    assert_ne!(now[0].reset_id, shown);
    assert_eq!(a.svc.devices().len(), 1, "old pin kept meanwhile");

    // What the user saw cannot pin what they did not see.
    assert_eq!(a.svc.confirm_trust_reset(&shown), Err(FsyncError::NotFound));
    assert_eq!(a.svc.decline_trust_reset(&shown), Err(FsyncError::NotFound));
    assert_eq!(
        a.svc.confirm_trust_reset(&now[0].reset_id).unwrap(),
        AccountState::NeedsPairing
    );
}

#[test]
fn the_initiator_never_sees_its_own_reset() {
    let _lan = lan_serial();
    let (_rt, be) = backend();
    let a = start_account(&be, 1, "dev-a");
    let b = logged_in(&be, 2, "dev-b", true);
    pair(&a, &b);
    assert_eq!(b.svc.ensure_account().unwrap(), AccountState::Ready);

    // A is pinned and online, B is its only other device.
    a.login(&be);
    assert_eq!(a.svc.reset_account_trust().unwrap(), AccountState::Created);
    assert_eq!(b.svc.ensure_account().unwrap(), AccountState::Ready);
    wait_for("TrustReset on B", || !b.rec.resets().is_empty());
    std::thread::sleep(Duration::from_millis(500)); // let any push reach A
    assert_eq!(a.svc.ensure_account().unwrap(), AccountState::Ready);
    assert!(a.rec.resets().is_empty());
    assert!(a.svc.pending_trust_resets().is_empty());
    assert_eq!(a.svc.devices().len(), 1);
    // The reset finished normally: B's offer is the only one.
    assert_eq!(b.svc.pending_trust_resets().len(), 1);
}

#[test]
fn reset_is_refused_for_a_pin_of_another_account_before_the_server_is_touched() {
    let (_rt, be) = backend();
    let a = start_account_with(&be, 1, "dev-a", false);
    // A second account with its own device.
    let other = "other@example.com";
    let e = dev_with(&be, 9, "dev-e", false);
    e.svc.login_start(other).unwrap();
    e.svc.login_verify(other, &be.last_code(other)).unwrap();
    assert_eq!(e.svc.ensure_account().unwrap(), AccountState::Created);

    // A signs in to the other account while still pinned to its own.
    a.svc.login_start(other).unwrap();
    a.svc.login_verify(other, &be.last_code(other)).unwrap();
    let err = a.svc.reset_account_trust().unwrap_err();
    assert!(
        matches!(&err, FsyncError::Invalid(m) if m.contains("account")),
        "{err:?}"
    );
    assert_eq!(a.svc.devices().len(), 1, "pin untouched");
    // The other account's list was not replaced.
    assert_eq!(e.svc.ensure_account().unwrap(), AccountState::Ready);
    assert!(e.svc.pending_trust_resets().is_empty());
    assert_eq!(e.rec.resets().len(), 0);
}

#[test]
fn repeating_a_reset_the_server_already_took_does_not_spend_the_daily_budget() {
    let (_rt, be) = backend();
    let _a = start_account_with(&be, 1, "dev-a", false);
    let c = logged_in(&be, 3, "dev-c", false);
    // The server allows 3 resets a day; a retry of a finished one must not count.
    for _ in 0..5 {
        assert_eq!(c.svc.reset_account_trust().unwrap(), AccountState::Created);
    }
    assert_eq!(c.svc.devices().len(), 1);
}

#[test]
fn alternating_resets_each_do_a_real_put_until_the_daily_budget_is_spent() {
    let (_rt, be) = backend();
    let _a = start_account_with(&be, 1, "dev-a", false);
    let (c, d) = (
        logged_in(&be, 3, "dev-c", false),
        logged_in(&be, 4, "dev-d", false),
    );
    // The server's list is the other device's root each time, so none is the no-PUT shortcut.
    for dev in [&c, &d, &c] {
        assert_eq!(
            dev.svc.reset_account_trust().unwrap(),
            AccountState::Created
        );
        let names: Vec<_> = dev.svc.devices().into_iter().map(|x| x.name).collect();
        assert_eq!(names.len(), 1, "{names:?}");
    }
    // 3 real resets used the whole budget (3 a day): the 4th is refused, the pin stays.
    let err = d.svc.reset_account_trust().unwrap_err();
    assert!(
        matches!(&err, FsyncError::Network(m) if m.contains("429")),
        "{err:?}"
    );
    assert_eq!(c.svc.devices().len(), 1);
}

#[test]
fn confirm_and_decline_racing_for_one_reset_never_report_invalid() {
    let (_rt, be) = backend();
    let a = start_account_with(&be, 1, "dev-a", false);
    let c = logged_in(&be, 3, "dev-c", false);
    c.svc.reset_account_trust().unwrap();
    a.svc.ensure_account().unwrap();
    let id = a.svc.pending_trust_resets()[0].reset_id.clone();

    let (confirm, decline) = std::thread::scope(|s| {
        let confirm = s.spawn(|| a.svc.confirm_trust_reset(&id));
        let decline = s.spawn(|| a.svc.decline_trust_reset(&id));
        (confirm.join().unwrap(), decline.join().unwrap())
    });
    match (confirm, decline) {
        (Ok(AccountState::NeedsPairing), Err(FsyncError::NotFound)) => {}
        (Err(FsyncError::NotFound), Ok(())) => {}
        other => panic!("exactly one answer wins, the other is NotFound: {other:?}"),
    }
}
