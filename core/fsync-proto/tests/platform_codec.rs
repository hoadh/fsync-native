//! Forward-compatible `Platform` codec (ADR 014): known variants keep the bytes of the old
//! derived enum, unknown indices decode and re-encode unchanged, chains with them verify.
mod common;
use common::*;
use fsync_proto::{
    DeviceEntry, DeviceList, Outcome, PairFrame, Platform, SignedDeviceList, sign, verify_chain,
    verify_genesis, verify_next,
};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn list_with(platforms: &[Platform]) -> DeviceList {
    let keys: Vec<_> = (1..=platforms.len() as u8).map(key).collect();
    DeviceList {
        v: 1,
        account_id: acct(),
        seq: 4,
        prev_hash: [5; 32],
        devices: platforms
            .iter()
            .zip(&keys)
            .enumerate()
            .map(|(i, (p, k))| DeviceEntry {
                node_id: nid(k),
                name: ["mac", "win", "cli", "lnx", "fut"][i].into(),
                platform: *p,
                added_seq: i as u64,
            })
            .collect(),
        signer: nid(&keys[0]),
        created_ms: 1_700_000_000_000,
    }
}

/// Bytes produced by the pre-change `#[derive(Serialize)] enum Platform { Mac, Win, Cli }`
/// for the same list (captured 2026-11-06 at c25c413).
const GOLDEN_MAC_WIN_CLI: &str = "0110000000000000000000000000000a11ce040505050505050505050505050505050505050505050505050505050505050505038a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c036d616300008139770ea87d175f56a35466c34c7ecccb8d8a91b4ee37a25df60f5b8fc9b3940377696e0101ed4928c628d1c2c6eae90338905995612959273a5c63f93636c14614ac8737d103636c6902028a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c80d095ffbc31";

#[test]
fn known_variants_keep_old_bytes() {
    let l = list_with(&[Platform::Mac, Platform::Win, Platform::Cli]);
    assert_eq!(hex(&l.to_bytes().unwrap()), GOLDEN_MAC_WIN_CLI);
    // PairFrame::DeviceInfo { name: "x", platform: Win } under the old derive.
    let f = PairFrame::DeviceInfo {
        name: "x".into(),
        platform: Platform::Win,
    };
    assert_eq!(hex(&postcard::to_stdvec(&f).unwrap()), "04017801");
}

#[test]
fn linux_and_unknown_round_trip_bytes() {
    let l = list_with(&[Platform::Mac, Platform::Linux, Platform::Unknown(9)]);
    let body = l.to_bytes().unwrap();
    let back: DeviceList = postcard::from_bytes(&body).unwrap();
    assert_eq!(back.devices[1].platform, Platform::Linux);
    assert_eq!(back.devices[2].platform, Platform::Unknown(9));
    assert_eq!(
        back.to_bytes().unwrap(),
        body,
        "re-encode must be byte-identical"
    );
    // A large index (multi-byte varint) survives too.
    let big = list_with(&[Platform::Unknown(70_000)]);
    let b = big.to_bytes().unwrap();
    assert_eq!(
        postcard::from_bytes::<DeviceList>(&b)
            .unwrap()
            .to_bytes()
            .unwrap(),
        b
    );
}

#[test]
fn wire_compat_maps_new_platforms_to_cli() {
    assert_eq!(Platform::Linux.wire_compat(), Platform::Cli);
    assert_eq!(Platform::Unknown(7).wire_compat(), Platform::Cli);
    assert_eq!(Platform::Win.wire_compat(), Platform::Win);
    assert_eq!(
        Platform::from_index(Platform::Unknown(12).index()),
        Platform::Unknown(12)
    );
}

#[test]
fn successor_keeps_unknown_index_and_chain_verifies() {
    let (a, b, c) = (key(1), key(2), key(3));
    let g = sign(
        &a,
        &DeviceList::genesis(acct(), nid(&a), "mac".into(), Platform::Mac, 1),
    )
    .unwrap();
    // A newer peer joins with a platform this build does not know.
    let l1 =
        DeviceList::next_add(&g, nid(&a), nid(&b), "fut".into(), Platform::Unknown(5), 2).unwrap();
    let s1 = sign(&a, &l1).unwrap();
    // This (tolerant, older) client adds another device: the unknown entry must survive.
    let l2 = DeviceList::next_add(&s1, nid(&a), nid(&c), "lnx".into(), Platform::Linux, 3).unwrap();
    let s2 = sign(&a, &l2).unwrap();
    let decoded = s2.decode().unwrap();
    let fut = decoded
        .devices
        .iter()
        .find(|d| d.node_id == nid(&b))
        .unwrap();
    assert_eq!(fut.platform, Platform::Unknown(5));

    verify_genesis(&g).unwrap();
    verify_next(&s1, &g).unwrap();
    match verify_chain(&g, &[s1.clone(), s2.clone()]).unwrap() {
        Outcome::Advanced(l) => assert_eq!(l.seq, 2),
        o => panic!("expected Advanced, got {o:?}"),
    }
    // Wire round trip of the signed list keeps the hash.
    let again = SignedDeviceList::from_bytes(&s2.to_bytes().unwrap()).unwrap();
    assert_eq!(again.hash().unwrap(), s2.hash().unwrap());
}

#[test]
fn pairing_device_info_with_linux_decodes() {
    let f = PairFrame::DeviceInfo {
        name: "x".into(),
        platform: Platform::Linux,
    };
    let bytes = postcard::to_stdvec(&f).unwrap();
    assert_eq!(hex(&bytes), "04017803");
    assert_eq!(postcard::from_bytes::<PairFrame>(&bytes).unwrap(), f);
}

#[test]
fn json_uses_variant_names() {
    assert_eq!(
        serde_json::to_string(&Platform::Linux).unwrap(),
        "\"Linux\""
    );
    assert_eq!(
        serde_json::from_str::<Platform>("\"Win\"").unwrap(),
        Platform::Win
    );
}
