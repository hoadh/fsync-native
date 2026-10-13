#![allow(unused_imports)]
mod common;
use common::*;
use fsync_proto::{
    DeviceList, Outcome, Platform, ProtoError, Sig, SignedDeviceList, sign, verify_chain,
    verify_chain_from_genesis, verify_genesis, verify_next,
};

#[test]
fn signed_roundtrip_and_hash_stable() {
    let g = genesis(&key(1));
    let bytes = g.to_bytes().unwrap();
    assert_eq!(SignedDeviceList::from_bytes(&bytes).unwrap(), g);
    assert_eq!(g.hash().unwrap(), genesis(&key(1)).hash().unwrap());
    assert_ne!(g.hash().unwrap(), genesis(&key(2)).hash().unwrap());
    let mut extra = bytes;
    extra.push(0);
    assert_eq!(
        SignedDeviceList::from_bytes(&extra),
        Err(ProtoError::TrailingBytes)
    );
}

#[test]
fn genesis_ok_and_shape_errors() {
    let (a, b) = (key(1), key(2));
    let l = verify_genesis(&genesis(&a)).unwrap();
    assert_eq!((l.seq, l.devices.len()), (0, 1));

    // seq != 0
    let g = genesis(&a);
    let n = add(&g, &a, &b);
    assert!(matches!(
        verify_genesis(&n),
        Err(ProtoError::InvalidGenesis(_))
    ));
    // non-zero prev_hash
    let mut l = DeviceList::genesis(acct(), nid(&a), "x".into(), Platform::Cli, 1);
    l.prev_hash = [1; 32];
    assert!(matches!(
        verify_genesis(&sign(&a, &l).unwrap()),
        Err(ProtoError::InvalidGenesis(_))
    ));
    // two devices
    let mut l = DeviceList::genesis(acct(), nid(&a), "x".into(), Platform::Cli, 1);
    l.devices.push(l.devices[0].clone());
    assert_eq!(
        verify_genesis(&sign(&a, &l).unwrap()),
        Err(ProtoError::DuplicateDevice)
    );
    // signer not the sole device
    let mut l = DeviceList::genesis(acct(), nid(&a), "x".into(), Platform::Cli, 1);
    l.devices[0].node_id = nid(&b);
    assert!(matches!(
        verify_genesis(&sign(&a, &l).unwrap()),
        Err(ProtoError::InvalidGenesis(_))
    ));
    // empty
    let mut l = DeviceList::genesis(acct(), nid(&a), "x".into(), Platform::Cli, 1);
    l.devices.clear();
    assert_eq!(
        verify_genesis(&sign(&a, &l).unwrap()),
        Err(ProtoError::EmptyList)
    );
    // bad version
    let mut l = DeviceList::genesis(acct(), nid(&a), "x".into(), Platform::Cli, 1);
    l.v = 9;
    assert_eq!(
        verify_genesis(&sign(&a, &l).unwrap()),
        Err(ProtoError::UnsupportedVersion(9))
    );
    // forged sig
    let mut g = genesis(&a);
    g.sig = Sig::from([7; 64]);
    assert_eq!(verify_genesis(&g), Err(ProtoError::BadSignature));
    // garbage body
    let g = SignedDeviceList {
        body: vec![0xff; 3],
        sig: Sig::from([0; 64]),
    };
    assert!(matches!(verify_genesis(&g), Err(ProtoError::Codec(_))));
}

#[test]
fn sign_requires_matching_key() {
    let l = DeviceList::genesis(acct(), nid(&key(1)), "x".into(), Platform::Mac, 1);
    assert_eq!(sign(&key(2), &l), Err(ProtoError::SignerMismatch));
}

#[test]
fn next_ok_identical_and_removal() {
    let (a, b) = (key(1), key(2));
    let g = genesis(&a);
    let n1 = add(&g, &a, &b);
    let Outcome::Advanced(l) = verify_next(&n1, &g).unwrap() else {
        panic!("expected advance")
    };
    assert_eq!((l.seq, l.devices.len(), l.devices[1].added_seq), (1, 2, 1));
    assert_eq!(l.prev_hash, g.hash().unwrap());
    // identical bytes at same seq: no-op
    assert_eq!(verify_next(&n1, &n1).unwrap(), Outcome::Unchanged);
    // newly added device can sign the next list (removing the first)
    let rm = DeviceList::next_remove(&n1, nid(&b), &nid(&a), 3).unwrap();
    let n2 = sign(&b, &rm).unwrap();
    let Outcome::Advanced(l) = verify_next(&n2, &n1).unwrap() else {
        panic!("expected advance")
    };
    assert_eq!(l.devices.len(), 1);
    assert!(!l.contains(&nid(&a)));
}

#[test]
fn constructor_errors() {
    let (a, b) = (key(1), key(2));
    let n1 = add(&genesis(&a), &a, &b);
    let dup = DeviceList::next_add(&n1, nid(&a), nid(&b), "x".into(), Platform::Cli, 1);
    assert_eq!(dup, Err(ProtoError::DuplicateDevice));
    assert_eq!(
        DeviceList::next_remove(&n1, nid(&a), &nid(&key(9)), 1),
        Err(ProtoError::DeviceNotFound)
    );
    let g = genesis(&a);
    assert_eq!(
        DeviceList::next_remove(&g, nid(&a), &nid(&a), 1),
        Err(ProtoError::EmptyList)
    );
}
