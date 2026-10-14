//! Trust reset through `FsyncService` against the real backend (ADR 012): a fresh device
//! re-creates the account's root, the old devices confirm or decline it, a confirmed device
//! pairs again.

mod backend_harness;
mod common;
mod service_harness;
use backend_harness::Backend;
use fsync_backend::auth::tokens::TokenService;
use fsync_core::service::*;
use service_harness::*;

#[test]
fn reset_then_confirm_decline_and_re_pair() {
    let _lan = lan_serial();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let be = rt.block_on(Backend::start());
    let a = start_account(&be, 1, "dev-a");
    let b = dev(&be, 2, "dev-b");
    b.login(&be);
    assert_eq!(b.svc.ensure_account().unwrap(), AccountState::NeedsPairing);
    pair(&a, &b);
    assert_eq!(b.svc.ensure_account().unwrap(), AccountState::Ready);
    assert_eq!(a.svc.devices().len(), 2);

    // C is a fresh identity with no way to pair: it needs the server list but no pin.
    let c = dev(&be, 3, "dev-c");
    c.login(&be);
    assert_eq!(c.svc.ensure_account().unwrap(), AccountState::NeedsPairing);
    assert!(c.svc.devices().is_empty());
    assert_eq!(c.svc.reset_account_trust().unwrap(), AccountState::Created);
    let cd = c.svc.devices();
    assert!(cd.len() == 1 && cd[0].is_self);
    c.svc.start().unwrap();

    // Unknown or malformed ids are NotFound, and nothing is pending before a sync.
    assert!(a.svc.pending_trust_resets().is_empty());
    for id in ["", "bogus", &"0".repeat(64), &"zz".repeat(32)] {
        assert_eq!(a.svc.confirm_trust_reset(id), Err(FsyncError::NotFound));
        assert_eq!(a.svc.decline_trust_reset(id), Err(FsyncError::NotFound));
    }

    // A syncs (a removed device's socket is closed, so it learns on its next sync).
    assert_eq!(a.svc.ensure_account().unwrap(), AccountState::Ready);
    wait_for("TrustReset event on A", || !a.rec.resets().is_empty());
    let pending = a.svc.pending_trust_resets();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].devices, 1);
    assert_eq!(pending[0].device_names, vec!["dev-c".to_string()]);
    assert_eq!(a.rec.reset_names(), vec![vec!["dev-c".to_string()]]);
    assert_eq!(pending[0].reset_id.len(), 64);
    assert_eq!(a.rec.resets(), vec![(pending[0].reset_id.clone(), 1)]);
    let id = pending[0].reset_id.clone();
    // Not announced twice; the pending entry stays until answered; the old pin still holds.
    assert_eq!(a.svc.ensure_account().unwrap(), AccountState::Ready);
    assert_eq!(a.svc.pending_trust_resets(), pending);
    assert_eq!(a.rec.resets().len(), 1);
    assert_eq!(a.svc.devices().len(), 2);
    // An id that is not pending stays NotFound.
    assert_eq!(
        a.svc.confirm_trust_reset(&"ab".repeat(32)),
        Err(FsyncError::NotFound)
    );

    // B declines: old pin kept, never offered again (not even once the chain moves on).
    b.svc.ensure_account().unwrap();
    wait_for("TrustReset event on B", || !b.rec.resets().is_empty());
    assert_eq!(
        b.rec.resets()[0].0,
        id,
        "same root, same id on every device"
    );
    b.svc.decline_trust_reset(&id).unwrap();
    assert!(b.svc.pending_trust_resets().is_empty());
    assert_eq!(b.svc.decline_trust_reset(&id), Err(FsyncError::NotFound));
    assert_eq!(b.svc.confirm_trust_reset(&id), Err(FsyncError::NotFound));
    assert_eq!(b.svc.ensure_account().unwrap(), AccountState::Ready);
    assert!(b.svc.pending_trust_resets().is_empty());
    assert_eq!(b.svc.devices().len(), 2, "old pin kept");
    assert_eq!(b.rec.resets().len(), 1);

    // A confirms: it is not in the new list -> SelfRemoved, NeedsPairing, nothing trusted.
    assert_eq!(
        a.svc.confirm_trust_reset(&id).unwrap(),
        AccountState::NeedsPairing
    );
    wait_for("SelfRemoved on A", || {
        a.rec.count(|e| matches!(e, AccountEvent::SelfRemoved)) == 1
    });
    assert!(a.svc.pending_trust_resets().is_empty());
    assert!(a.svc.devices().is_empty(), "old pin dropped");
    assert_eq!(a.svc.confirm_trust_reset(&id), Err(FsyncError::NotFound));
    assert_eq!(a.svc.ensure_account().unwrap(), AccountState::NeedsPairing);

    // C pairs A again with the normal flow.
    pair(&c, &a);
    assert_eq!(a.svc.ensure_account().unwrap(), AccountState::Ready);
    assert_eq!(a.svc.devices().len(), 2);
    assert_eq!(c.svc.devices().len(), 2);

    // B declined the root, which stays declined although the chain moved on.
    assert_eq!(b.svc.ensure_account().unwrap(), AccountState::Ready);
    assert!(b.svc.pending_trust_resets().is_empty());
    assert_eq!(b.rec.resets().len(), 1);
    assert_eq!(b.svc.devices().len(), 2);
}

#[test]
fn reset_needs_a_session_a_list_and_a_recent_login() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let be = rt.block_on(Backend::start());

    // No session.
    let c = dev_with(&be, 3, "dev-c", false);
    assert_eq!(c.svc.reset_account_trust(), Err(FsyncError::Auth));

    // Signed in but the account has no list: nothing to reset.
    let tokens = c.login(&be);
    assert!(matches!(
        c.svc.reset_account_trust(),
        Err(FsyncError::Invalid(_))
    ));

    let a = start_account_with(&be, 1, "dev-a", false);
    // Same live session family, but the login claims to be 10 minutes old.
    let ts = TokenService::new(&[42; 32]);
    let claims = ts.verify_access(&tokens.access).unwrap();
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap();
    let stale = ts
        .issue_with(claims.account, claims.sid, now - 600, now, now + 900)
        .unwrap();
    c.svc
        .set_session(SessionTokens {
            access: stale,
            ..tokens
        })
        .unwrap();
    assert_eq!(c.svc.reset_account_trust(), Err(FsyncError::ReauthRequired));
    assert!(c.svc.devices().is_empty(), "nothing pinned on failure");
    // The server list is untouched: A syncs without seeing a reset.
    assert_eq!(a.svc.ensure_account().unwrap(), AccountState::Ready);
    assert!(a.svc.pending_trust_resets().is_empty());

    // Signing in again fixes it.
    c.login(&be);
    assert_eq!(c.svc.reset_account_trust().unwrap(), AccountState::Created);
}
