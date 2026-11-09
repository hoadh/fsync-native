//! ADR 014: the backend accepts device lists carrying `Platform::Linux` or a platform index it
//! does not know (forward-compatible decoder), as genesis and as a successor.
use ed25519_dalek::SigningKey;
use fsync_backend::devices::verify::{Decision, decide};
use fsync_proto::{DeviceList, NodeId, Platform, sign};
use uuid::Uuid;

fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}

#[test]
fn accepts_linux_genesis_and_unknown_successor() {
    let account = Uuid::from_u128(0xBEEF);
    let (a, b) = (key(1), key(2));
    let (na, nb) = (NodeId::from_signing_key(&a), NodeId::from_signing_key(&b));
    let g = sign(
        &a,
        &DeviceList::genesis(account, na, "lnx".into(), Platform::Linux, 1),
    )
    .unwrap();
    assert!(matches!(
        decide(account, None, &g, false),
        Ok(Decision::Genesis(_))
    ));
    let next = DeviceList::next_add(&g, na, nb, "fut".into(), Platform::Unknown(11), 2).unwrap();
    let s1 = sign(&a, &next).unwrap();
    match decide(account, Some(&g), &s1, false) {
        Ok(Decision::Next(l)) => assert_eq!(l.devices[1].platform, Platform::Unknown(11)),
        other => panic!("expected Next, got {other:?}"),
    }
}
