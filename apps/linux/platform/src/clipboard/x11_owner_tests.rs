use super::latin1;

#[test]
fn string_target_is_latin1_only_when_every_char_fits() {
    assert_eq!(
        latin1("caf\u{e9}".as_bytes()),
        Some(vec![b'c', b'a', b'f', 0xe9])
    );
    assert_eq!(latin1("zażółć".as_bytes()), None);
    assert_eq!(latin1(&[0xff, 0xfe]), None);
}
