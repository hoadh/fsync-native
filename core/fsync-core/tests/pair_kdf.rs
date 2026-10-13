//! pair/1 key schedule: SAS, QR KDF, MAC verification.

use fsync_core::pair::{PairKeys, Role, qr_key, transcript};
use fsync_proto::{NodeId, pairing::PairMode};

fn ids() -> (NodeId, NodeId) {
    (NodeId([1; 32]), NodeId([2; 32]))
}

#[test]
fn sas_is_deterministic_three_words_and_depends_on_key() {
    let (e, n) = ids();
    let t = transcript(PairMode::Code, &e, &n, (&[0; 16], &[1; 16]), b"a", b"b");
    let k1 = PairKeys::derive(b"shared-1", &t);
    let k2 = PairKeys::derive(b"shared-1", &t);
    let k3 = PairKeys::derive(b"shared-2", &t);
    assert_eq!(k1.sas(), k2.sas());
    assert_ne!(k1.sas(), k3.sas());
    let words: Vec<_> = k1.sas().split(' ').map(str::to_owned).collect();
    assert_eq!(words.len(), 3);
    for w in words {
        assert_eq!(w.len(), 4);
        assert!(w.chars().all(|c| c.is_ascii_lowercase()));
    }
}

#[test]
fn transcript_binds_mode_ids_and_messages() {
    let (e, n) = ids();
    let base = transcript(PairMode::Code, &e, &n, (&[0; 16], &[1; 16]), b"a", b"b");
    assert_ne!(
        base,
        transcript(PairMode::Qr, &e, &n, (&[0; 16], &[1; 16]), b"a", b"b")
    );
    assert_ne!(
        base,
        transcript(PairMode::Code, &n, &e, (&[0; 16], &[1; 16]), b"a", b"b")
    );
    assert_ne!(
        base,
        transcript(PairMode::Code, &e, &n, (&[0; 16], &[1; 16]), b"b", b"a")
    );
    // Length-prefixing: moving a byte across the boundary changes the hash.
    assert_ne!(
        transcript(PairMode::Code, &e, &n, (&[0; 16], &[1; 16]), b"ab", b""),
        transcript(PairMode::Code, &e, &n, (&[0; 16], &[1; 16]), b"a", b"b")
    );
}

#[test]
fn transcript_binds_nonces_so_qr_sessions_are_fresh() {
    let (e, n) = ids();
    let t = |ne: u8, nn: u8| transcript(PairMode::Qr, &e, &n, (&[ne; 16], &[nn; 16]), b"", b"");
    assert_ne!(t(1, 2), t(3, 2));
    assert_ne!(t(1, 2), t(1, 3));
    assert_ne!(t(1, 2), t(2, 1));
    // Same QR secret, different nonces -> different MAC key and SAS.
    let key = qr_key(&[7; 16], &e, &n);
    let (a, b) = (
        PairKeys::derive(&*key, &t(1, 2)),
        PairKeys::derive(&*key, &t(4, 5)),
    );
    assert_ne!(a.sas(), b.sas());
    assert_ne!(a.mac(Role::New, &t(1, 2)), b.mac(Role::New, &t(4, 5)));
}

#[test]
fn qr_key_depends_on_secret_and_both_ids() {
    let (e, n) = ids();
    let k = qr_key(&[7; 16], &e, &n);
    assert_eq!(*k, *qr_key(&[7; 16], &e, &n));
    assert_ne!(*k, *qr_key(&[8; 16], &e, &n));
    assert_ne!(*k, *qr_key(&[7; 16], &n, &e));
}

#[test]
fn mac_mismatch_and_reflection_fail() {
    let (e, n) = ids();
    let t = transcript(PairMode::Code, &e, &n, (&[0; 16], &[1; 16]), b"a", b"b");
    let good = PairKeys::derive(b"k", &t);
    let bad = PairKeys::derive(b"other", &t);
    let mac = good.mac(Role::New, &t);
    assert!(good.verify(Role::New, &t, &mac));
    assert!(
        !good.verify(Role::Existing, &t, &mac),
        "role separation blocks reflection"
    );
    assert!(!bad.verify(Role::New, &t, &mac), "wrong key");
    let mut flipped = mac;
    flipped[31] ^= 1;
    assert!(!good.verify(Role::New, &t, &flipped));
    assert!(!good.verify(Role::New, &[0; 32], &mac), "wrong transcript");
}
