#![allow(unused_imports)]
mod common;
use common::*;
use fsync_proto::{
    DeviceList, Outcome, Platform, ProtoError, Sig, SignedDeviceList, sign, verify_chain,
    verify_chain_from_genesis, verify_genesis, verify_next,
};

#[test]
fn rollback_rejected() {
    let (a, b, c) = (key(1), key(2), key(3));
    let g = genesis(&a);
    let n1 = add(&g, &a, &b);
    let n2 = add(&n1, &a, &c);
    assert_eq!(
        verify_next(&n1, &n2),
        Err(ProtoError::Rollback { got: 1, pinned: 2 })
    );
    // different list at the same seq is also a rollback/fork
    let n1b = add(&g, &a, &c);
    assert_eq!(
        verify_next(&n1b, &n1),
        Err(ProtoError::Rollback { got: 1, pinned: 1 })
    );
}

#[test]
fn gap_and_chain() {
    let (a, b, c, d) = (key(1), key(2), key(3), key(4));
    let g = genesis(&a);
    let n1 = add(&g, &a, &b);
    let n2 = add(&n1, &b, &c);
    let n3 = add(&n2, &c, &d);
    assert_eq!(
        verify_next(&n3, &g),
        Err(ProtoError::Gap {
            expected: 1,
            got: 3
        })
    );
    let Outcome::Advanced(l) = verify_chain(&g, &[n1.clone(), n2.clone(), n3.clone()]).unwrap()
    else {
        panic!("expected advance")
    };
    assert_eq!((l.seq, l.devices.len()), (3, 4));
    // replay of already-pinned head inside a chain is skipped
    assert_eq!(verify_chain(&g, &[]).unwrap(), Outcome::Unchanged);
    assert_eq!(
        verify_chain(&n1, std::slice::from_ref(&n1)).unwrap(),
        Outcome::Unchanged
    );
    // chain with a hole fails
    assert!(matches!(
        verify_chain(&g, &[n1.clone(), n3.clone()]),
        Err(ProtoError::Gap { .. })
    ));
    // from genesis
    assert_eq!(
        verify_chain_from_genesis(&[g.clone(), n1, n2]).unwrap().seq,
        2
    );
    assert_eq!(verify_chain_from_genesis(&[g]).unwrap().seq, 0);
    assert!(matches!(
        verify_chain_from_genesis(&[]),
        Err(ProtoError::InvalidGenesis(_))
    ));
    assert!(matches!(
        verify_chain_from_genesis(&[n3]),
        Err(ProtoError::InvalidGenesis(_))
    ));
}

#[test]
fn wrong_account() {
    let a = key(1);
    let g = genesis(&a);
    let mut l = g.decode().unwrap();
    l.seq = 1;
    l.account_id = uuid::Uuid::from_u128(1);
    l.prev_hash = g.hash().unwrap();
    assert_eq!(
        verify_next(&sign(&a, &l).unwrap(), &g),
        Err(ProtoError::WrongAccount)
    );
}

#[test]
fn signer_not_trusted() {
    let (a, b, evil) = (key(1), key(2), key(66));
    let g = genesis(&a);
    let l = DeviceList::next_add(&g, nid(&evil), nid(&b), "x".into(), Platform::Cli, 1).unwrap();
    assert_eq!(
        verify_next(&sign(&evil, &l).unwrap(), &g),
        Err(ProtoError::SignerNotTrusted)
    );
}

#[test]
fn forged_signature_and_tampered_body() {
    let (a, b, evil) = (key(1), key(2), key(66));
    let g = genesis(&a);
    // signed by evil but claims signer = a
    let l = DeviceList::next_add(&g, nid(&a), nid(&b), "x".into(), Platform::Cli, 1).unwrap();
    let mut forged = sign(&a, &l).unwrap();
    forged.sig = sign_with(&evil, &forged.body);
    assert_eq!(verify_next(&forged, &g), Err(ProtoError::BadSignature));
    // tampered body
    let mut t = add(&g, &a, &b);
    let last = t.body.len() - 1;
    t.body[last] ^= 1;
    assert!(verify_next(&t, &g).is_err());
    // wrong domain: raw signature over body without the domain prefix
    let mut raw = add(&g, &a, &b);
    raw.sig = sign_with(&a, &raw.body);
    assert_eq!(verify_next(&raw, &g), Err(ProtoError::BadSignature));
}

fn sign_with(k: &ed25519_dalek::SigningKey, body: &[u8]) -> Sig {
    use ed25519_dalek::Signer;
    Sig::from(k.sign(body).to_bytes())
}

#[test]
fn prev_hash_mismatch() {
    let (a, b) = (key(1), key(2));
    let g = genesis(&a);
    let mut l = DeviceList::next_add(&g, nid(&a), nid(&b), "x".into(), Platform::Cli, 1).unwrap();
    l.prev_hash = [9; 32];
    assert_eq!(
        verify_next(&sign(&a, &l).unwrap(), &g),
        Err(ProtoError::PrevHashMismatch)
    );
}

#[test]
fn trust_reset_on_new_genesis() {
    let (a, b) = (key(1), key(2));
    let g = genesis(&a);
    let n1 = add(&g, &a, &b);
    let new_root = genesis(&b); // same account, different root
    assert_eq!(verify_next(&new_root, &g), Err(ProtoError::TrustReset));
    assert_eq!(verify_next(&new_root, &n1), Err(ProtoError::TrustReset));
    // seq-0 list that is not a valid genesis is not reported as reset
    let mut l = new_root.decode().unwrap();
    l.prev_hash = [1; 32];
    let bad = sign(&b, &l).unwrap();
    assert!(matches!(
        verify_next(&bad, &g),
        Err(ProtoError::InvalidGenesis(_))
    ));
}

#[test]
fn structurally_invalid_successor() {
    let (a, b) = (key(1), key(2));
    let g = genesis(&a);
    let mut l = DeviceList::next_add(&g, nid(&a), nid(&b), "x".into(), Platform::Cli, 1).unwrap();
    l.devices.push(l.devices[0].clone());
    assert_eq!(
        verify_next(&sign(&a, &l).unwrap(), &g),
        Err(ProtoError::DuplicateDevice)
    );
}
