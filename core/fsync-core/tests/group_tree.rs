//! The chunk-group CV list must reproduce blake3 exactly, for every file shape.

use fsync_core::file::{GROUP, Verifier, digest_read, group_count, root_of};

fn data(n: usize) -> Vec<u8> {
    (0..n)
        .map(|i| (i.wrapping_mul(31) ^ (i >> 7)) as u8)
        .collect()
}

#[test]
fn root_matches_blake3_for_many_sizes() {
    let g = GROUP;
    let sizes = [
        0,
        1,
        1023,
        1024,
        1025,
        g - 1,
        g,
        g + 1,
        2 * g,
        3 * g,
        3 * g + 5,
        5 * g,
        7 * g + 1,
        8 * g,
        9 * g - 1,
        16 * g + 3,
        33 * g,
        100 * g + 77,
    ];
    for n in sizes {
        let d = data(n);
        let dg = digest_read(d.as_slice()).unwrap();
        assert_eq!(dg.hash, *blake3::hash(&d).as_bytes(), "size {n}");
        assert_eq!(dg.size, n as u64);
        // Single-group files need no list: the file hash is the group hash.
        let want = if n <= g { 0 } else { group_count(n as u64) };
        assert_eq!(dg.cvs.len() as u64, want, "cv count {n}");
        if n > g {
            assert_eq!(root_of(&dg.cvs), Some(dg.hash), "root_of size {n}");
        }
    }
}

#[test]
fn verifier_accepts_good_and_rejects_flipped_group() {
    let g = GROUP;
    let mut d = data(5 * g + 10);
    let dg = digest_read(d.as_slice()).unwrap();
    let v = Verifier::new(dg.hash, dg.size, dg.cvs.clone()).unwrap();
    for i in 0..6 {
        let s = i * g;
        let e = (s + g).min(d.len());
        assert!(v.check(s as u64, &d[s..e]));
    }
    d[2 * g + 5] ^= 1;
    assert!(!v.check(2 * g as u64, &d[2 * g..3 * g]));
    // A CV list that does not hash to the announced root is refused.
    let mut bad = dg.cvs.clone();
    bad[1][0] ^= 1;
    assert!(Verifier::new(dg.hash, dg.size, bad).is_none());
}
