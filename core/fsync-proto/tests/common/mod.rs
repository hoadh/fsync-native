#![allow(dead_code)]
use ed25519_dalek::SigningKey;
use fsync_proto::{
    ClipKind, ClipOffer, DeviceList, FileMeta, NodeId, Platform, RepMeta, SignedDeviceList, sign,
};
use uuid::Uuid;

pub fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
pub fn nid(k: &SigningKey) -> NodeId {
    NodeId::from_signing_key(k)
}
pub fn acct() -> Uuid {
    Uuid::from_u128(0xA11CE)
}
pub fn genesis(k: &SigningKey) -> SignedDeviceList {
    let l = DeviceList::genesis(acct(), nid(k), "mac".into(), Platform::Mac, 1);
    sign(k, &l).unwrap()
}
/// `signer` adds `new` on top of `prev`.
pub fn add(prev: &SignedDeviceList, signer: &SigningKey, new: &SigningKey) -> SignedDeviceList {
    let l =
        DeviceList::next_add(prev, nid(signer), nid(new), "dev".into(), Platform::Win, 2).unwrap();
    sign(signer, &l).unwrap()
}

pub fn offer() -> ClipOffer {
    let bytes = b"hello".to_vec();
    ClipOffer {
        v: 1,
        item_id: Uuid::from_u128(5),
        origin: nid(&key(1)),
        ts_ms: 10,
        kind: ClipKind::Text,
        reps: vec![RepMeta {
            mime: "text/plain".into(),
            size: 5,
            hash: *blake3::hash(&bytes).as_bytes(),
        }],
        inline: vec![Some(bytes)],
        files: vec![],
        preview: Some("hello".into()),
        push: false,
        sensitive: false,
    }
}

pub fn file(id: u32, rel: &str) -> FileMeta {
    FileMeta {
        file_id: id,
        name: "a".into(),
        rel_path: rel.into(),
        size: 1,
        hash: [0; 32],
        mtime_ms: 0,
    }
}
