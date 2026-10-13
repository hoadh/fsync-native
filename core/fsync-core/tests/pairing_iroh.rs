//! pair/1 end to end over real iroh endpoints (loopback), in-memory backend.

mod common;
use common::{
    MemBackend, Peer,
    pairing::{Outcome, pair},
    peer,
};
use fsync_core::{CoreError, PairError, PairSecret};
use fsync_proto::pairing::AbortReason;
use uuid::Uuid;
use zeroize::Zeroizing;

fn code(c: &str) -> PairSecret {
    PairSecret::Code(Zeroizing::new(c.to_owned()))
}

fn assert_paired(e: &Peer, n: &Peer, out: &Outcome, backend: &MemBackend) {
    assert_eq!(*out.existing.as_ref().unwrap(), 1);
    assert_eq!(*out.new.as_ref().unwrap(), 1);
    assert!(
        out.sas_existing.is_some() && out.sas_existing == out.sas_new,
        "SAS must match on both screens"
    );
    assert_eq!(n.trust.list().unwrap().seq, 1);
    assert_eq!(e.trust.list().unwrap().seq, 1);
    assert!(n.trust.is_trusted(&e.nid()) && n.trust.is_trusted(&n.nid()));
    assert!(e.trust.is_trusted(&n.nid()));
    // N pinned only the head signed by E: genesis never seen, root unknown.
    assert_eq!(n.trust.root_hash(), Some(fsync_core::trust::UNKNOWN_ROOT));
    assert_eq!(
        backend.0.lock().unwrap().len(),
        2,
        "list published to backend"
    );
}

#[tokio::test]
async fn code_mode_with_sas_accept_pins_seq1_on_both() {
    let (mut e, mut n) = (peer().await, peer().await);
    let (out, backend) = pair(&mut e, &mut n, code("123456"), code("123456"), true).await;
    assert_paired(&e, &n, &out, &backend);
    // New device may now be dialed by E (trusted), and vice versa after discovery.
    assert!(e.node.connect(&n.nid(), common::TEST_ALPN).await.is_ok());
}

#[tokio::test]
async fn qr_mode_pairs_and_still_shows_sas() {
    let (mut e, mut n) = (peer().await, peer().await);
    let s = [9u8; 16];
    let (out, backend) = pair(
        &mut e,
        &mut n,
        PairSecret::Qr(Zeroizing::new(s)),
        PairSecret::Qr(Zeroizing::new(s)),
        true,
    )
    .await;
    assert_paired(&e, &n, &out, &backend);
}

#[tokio::test]
async fn wrong_code_aborts_on_bad_mac_and_pins_nothing() {
    let (mut e, mut n) = (peer().await, peer().await);
    let (out, backend) = pair(&mut e, &mut n, code("111111"), code("222222"), true).await;
    // One wrong guess does not burn the window (it just runs into its TTL here).
    assert!(
        matches!(out.new, Err(CoreError::Pair(PairError::Timeout))),
        "new: {:?}",
        out.new
    );
    assert!(matches!(
        out.existing,
        Err(CoreError::Pair(PairError::PeerAborted(AbortReason::BadMac)))
    ));
    assert!(
        out.sas_existing.is_none(),
        "no confirm prompt after a failed MAC"
    );
    assert!(n.trust.head().is_none());
    assert_eq!(e.trust.list().unwrap().seq, 0);
    assert_eq!(backend.0.lock().unwrap().len(), 1);
    // Same source gets one unclaimed attempt per window: a retry gets nowhere.
    let again = e
        .node
        .pair_with(&n.nid(), None, &[n.loopback()], &code("222222"), &backend)
        .await;
    assert!(again.is_err());
}

#[tokio::test]
async fn qr_secret_mismatch_aborts() {
    let (mut e, mut n) = (peer().await, peer().await);
    let (out, _b) = pair(
        &mut e,
        &mut n,
        PairSecret::Qr(Zeroizing::new([1; 16])),
        PairSecret::Qr(Zeroizing::new([2; 16])),
        true,
    )
    .await;
    assert!(matches!(out.new, Err(CoreError::Pair(PairError::Timeout))));
    assert!(matches!(
        out.existing,
        Err(CoreError::Pair(PairError::PeerAborted(AbortReason::BadMac)))
    ));
    assert!(n.trust.head().is_none());
}

#[tokio::test]
async fn mode_downgrade_is_refused() {
    let (mut e, mut n) = (peer().await, peer().await);
    let (out, _b) = pair(
        &mut e,
        &mut n,
        PairSecret::Qr(Zeroizing::new([1; 16])),
        code("123456"),
        true,
    )
    .await;
    assert!(out.new.is_err() && out.existing.is_err());
    assert!(n.trust.head().is_none());
}

#[tokio::test]
async fn user_rejection_leaves_both_sides_unchanged() {
    let (mut e, mut n) = (peer().await, peer().await);
    let (out, backend) = pair(&mut e, &mut n, code("654321"), code("654321"), false).await;
    assert!(
        matches!(out.existing, Err(CoreError::Pair(PairError::Rejected))),
        "{:?}",
        out.existing
    );
    assert!(out.new.is_err());
    assert!(n.trust.head().is_none());
    assert!(!e.trust.is_trusted(&n.nid()));
    assert_eq!(backend.0.lock().unwrap().len(), 1, "nothing published");
}

#[tokio::test]
async fn pair_without_open_window_is_refused() {
    let (e, n) = (peer().await, peer().await);
    let backend = MemBackend::with(e.genesis(Uuid::new_v4()));
    let r = e
        .node
        .pair_with(&n.nid(), None, &[n.loopback()], &code("123456"), &backend)
        .await;
    assert!(r.is_err());
}
