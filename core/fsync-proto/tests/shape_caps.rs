mod common;
use common::*;
use fsync_proto::{
    DeviceList, Platform, ProtoError, device_list::DeviceEntry, shape::MAX_DEVICES, sign,
    verify_genesis, verify_next,
};

#[test]
fn verify_enforces_device_list_shape() {
    let k = key(1);
    // Bad name in a genesis list.
    let l = DeviceList::genesis(acct(), nid(&k), "evil\u{202E}".into(), Platform::Mac, 1);
    assert!(matches!(
        verify_genesis(&sign(&k, &l).unwrap()),
        Err(ProtoError::InvalidDeviceList(_))
    ));

    // Too many devices: build seq-1 by hand (the builder itself refuses past the cap).
    let g = genesis(&k);
    let mut big = g.decode().unwrap();
    big.seq = 1;
    big.prev_hash = g.hash().unwrap();
    for i in 0..MAX_DEVICES as u8 {
        big.devices.push(DeviceEntry {
            node_id: nid(&key(10 + i)),
            name: format!("d{i}"),
            platform: Platform::Cli,
            added_seq: 1,
        });
    }
    assert_eq!(big.devices.len(), MAX_DEVICES + 1);
    let signed = sign(&k, &big).unwrap();
    assert!(matches!(
        verify_next(&signed, &g),
        Err(ProtoError::InvalidDeviceList("too many devices"))
    ));

    // The builder refuses the 33rd device too.
    let mut cur = g;
    for i in 1..MAX_DEVICES as u8 {
        cur = add(&cur, &k, &key(100 + i));
    }
    assert_eq!(cur.decode().unwrap().devices.len(), MAX_DEVICES);
    let over = DeviceList::next_add(&cur, nid(&k), nid(&key(200)), "x".into(), Platform::Win, 3);
    assert!(matches!(
        over,
        Err(ProtoError::InvalidDeviceList("too many devices"))
    ));
}

#[test]
fn seq_overflow_is_an_error_not_a_panic() {
    let k = key(1);
    let mut l = DeviceList::genesis(acct(), nid(&k), "m".into(), Platform::Mac, 1);
    l.seq = u64::MAX;
    let signed = sign(&k, &l).unwrap();
    let r = DeviceList::next_add(&signed, nid(&k), nid(&key(2)), "n".into(), Platform::Win, 2);
    assert_eq!(r.unwrap_err(), ProtoError::SeqOverflow);
}
