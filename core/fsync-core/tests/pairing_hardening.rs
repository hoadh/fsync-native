//! pair/1 hardening: consent on the new device, guess limits, window lifecycle.

mod common;
use common::{
    MemBackend,
    pairing::{answer_confirm, new_info, pair_decisions},
    peer,
};
use fsync_core::{CoreError, PairError, PairSecret};
use std::time::Duration;
use uuid::Uuid;
use zeroize::Zeroizing;

fn code(c: &str) -> PairSecret {
    PairSecret::Code(Zeroizing::new(c.to_owned()))
}

#[tokio::test]
async fn new_device_rejecting_pins_nothing_on_either_side() {
    // E's user accepts, N's user rejects: E must not publish, nobody pins (code + QR).
    for secret in [code("654321"), PairSecret::Qr(Zeroizing::new([5; 16]))] {
        let (mut e, mut n) = (peer().await, peer().await);
        let (out, backend) =
            pair_decisions(&mut e, &mut n, secret.clone(), secret, true, false).await;
        assert!(
            matches!(out.new, Err(CoreError::Pair(PairError::Rejected))),
            "{:?}",
            out.new
        );
        assert!(out.existing.is_err());
        assert!(n.trust.head().is_none(), "N must not pin without consent");
        assert_eq!(e.trust.list().unwrap().seq, 0);
        assert!(!e.trust.is_trusted(&n.nid()));
        assert_eq!(backend.0.lock().unwrap().len(), 1, "nothing published");
    }
}

#[tokio::test]
async fn attacker_with_code_cannot_get_pinned_when_user_rejects() {
    // The backend knows the code and dials N as "E": SPAKE2 passes, N's user rejects.
    let (mut attacker, mut n) = (peer().await, peer().await);
    let (c1, c2) = (code("123456"), code("123456"));
    let (out, _b) = pair_decisions(&mut attacker, &mut n, c1, c2, true, false).await;
    assert!(matches!(out.new, Err(CoreError::Pair(PairError::Rejected))));
    assert!(n.trust.head().is_none());
    assert!(!n.trust.is_trusted(&attacker.nid()));
}

#[tokio::test]
async fn unanswered_prompt_on_new_device_pins_nothing() {
    // Dropping the PairDecision without answering is a negative decision.
    let (mut e, mut n) = (peer().await, peer().await);
    let backend = MemBackend::with(e.genesis(Uuid::new_v4()));
    let (n_id, addr, secret) = (n.nid(), [n.loopback()], code("111222"));
    let new_fut = n
        .node
        .await_pairing(secret.clone(), new_info(), Duration::from_secs(20));
    let e_fut = e.node.pair_with(&n_id, None, &addr, &secret, &backend);
    let (rn, re, _, _) = tokio::join!(
        new_fut,
        e_fut,
        answer_confirm(&mut n.events, false),
        answer_confirm(&mut e.events, true)
    );
    assert!(rn.is_err() && re.is_err());
    assert!(n.trust.head().is_none());
    assert_eq!(backend.0.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn failed_guess_does_not_burn_window_for_the_real_device() {
    // M1: an attacker's wrong-code attempt leaves the window open for E (3 failures max).
    let (mut e, mut n, attacker) = (peer().await, peer().await, peer().await);
    let backend = MemBackend::with(e.genesis(Uuid::new_v4()));
    let abackend = MemBackend::with(attacker.genesis(Uuid::new_v4()));
    let (n_id, addr, good) = (n.nid(), [n.loopback()], code("424242"));
    let new_fut = n
        .node
        .await_pairing(good.clone(), new_info(), Duration::from_secs(20));
    let both = async {
        let bad = attacker
            .node
            .pair_with(&n_id, None, &addr, &code("000000"), &abackend)
            .await;
        assert!(bad.is_err());
        e.node.pair_with(&n_id, None, &addr, &good, &backend).await
    };
    let (rn, re, _, _) = tokio::join!(
        new_fut,
        both,
        answer_confirm(&mut n.events, true),
        answer_confirm(&mut e.events, true)
    );
    assert_eq!(rn.unwrap(), 1);
    assert_eq!(re.unwrap(), 1);
}

#[tokio::test]
async fn three_wrong_guesses_close_the_window() {
    let n = peer().await;
    let (n_id, addr) = (n.nid(), [n.loopback()]);
    let new_fut = n
        .node
        .await_pairing(code("424242"), new_info(), Duration::from_secs(20));
    let guesses = async {
        for i in 0..fsync_core::pair::MAX_FAILED {
            let a = peer().await;
            let b = MemBackend::with(a.genesis(Uuid::new_v4()));
            let c = code(&format!("00000{i}"));
            assert!(a.node.pair_with(&n_id, None, &addr, &c, &b).await.is_err());
        }
    };
    let (r, _) = tokio::join!(new_fut, guesses);
    assert!(
        matches!(r, Err(CoreError::Pair(PairError::BadMac))),
        "{r:?}"
    );
}

#[tokio::test]
async fn await_pairing_times_out_when_unclaimed() {
    let n = peer().await;
    let r = n
        .node
        .await_pairing(code("123123"), new_info(), Duration::from_millis(300))
        .await;
    assert!(matches!(r, Err(CoreError::Pair(PairError::Timeout))));
}
