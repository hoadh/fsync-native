#![allow(unused_imports)]
mod common;
use common::*;
use fsync_proto::{
    ALPN_CLIP, ALPN_FILE, ALPN_PAIR, ClipKind, ClipOffer, FetchReq, FetchResp, FetchStatus,
    FetchTarget, FileMeta, NodeId, Nonce, PairFrame, Platform, ProtoError, QrPayload, RepMeta,
    api_dto::*,
    clip::{MAX_INLINE_TOTAL, validate_rel_path},
    file::CHUNK_GROUP,
    frame::*,
    pairing::{AbortReason, PairMode},
    presence::*,
};
use uuid::Uuid;

#[test]
fn clip_offer_validation() {
    let o = offer();
    assert_eq!(o.validate(), Ok(()));
    let (back, _): (ClipOffer, usize) = decode_frame(&encode_frame(&o).unwrap()).unwrap();
    assert_eq!(back, o);
    let err = |f: &dyn Fn(&mut ClipOffer)| {
        let mut o = offer();
        f(&mut o);
        o.validate().unwrap_err()
    };
    assert_eq!(err(&|o| o.v = 2), ProtoError::UnsupportedVersion(2));
    assert!(matches!(
        err(&|o| o.inline.clear()),
        ProtoError::InvalidOffer(_)
    ));
    assert!(matches!(
        err(&|o| o.reps[0].size = 9),
        ProtoError::InvalidOffer(_)
    ));
    assert!(matches!(
        err(&|o| o.reps[0].hash = [0; 32]),
        ProtoError::InvalidOffer(_)
    ));
    assert!(matches!(
        err(&|o| o.preview = Some("x".repeat(201))),
        ProtoError::InvalidOffer(_)
    ));
    assert!(matches!(
        err(&|o| o.files.push(file(1, "a"))),
        ProtoError::InvalidOffer(_)
    ));
    assert!(matches!(
        err(&|o| o.kind = ClipKind::Files),
        ProtoError::InvalidOffer(_)
    ));
    // inline budget
    let big = vec![1u8; MAX_INLINE_TOTAL + 1];
    let e = err(&|o| {
        o.reps[0].size = big.len() as u64;
        o.reps[0].hash = *blake3::hash(&big).as_bytes();
        o.inline[0] = Some(big.clone());
    });
    assert!(matches!(
        e,
        ProtoError::InvalidOffer("inline budget exceeded")
    ));
    // non-inline rep is fine
    let mut o2 = offer();
    o2.inline[0] = None;
    assert_eq!(o2.validate(), Ok(()));
    // files
    let mut f = offer();
    f.kind = ClipKind::Files;
    f.push = true;
    f.files = vec![file(1, "dir/a.txt"), file(2, "b")];
    assert_eq!(f.validate(), Ok(()));
    f.files[1].file_id = 1;
    assert!(matches!(
        f.validate(),
        Err(ProtoError::InvalidOffer("duplicate file_id"))
    ));
    f.files[1].file_id = 2;
    f.files[1].rel_path = "../x".into();
    assert_eq!(f.validate(), Err(ProtoError::InvalidPath));
}

#[test]
fn rel_path_rules() {
    for ok in ["a", "a/b.txt", "a b/c"] {
        assert_eq!(validate_rel_path(ok), Ok(()), "{ok}");
    }
    for bad in [
        "", "/a", "a//b", "a/", "../a", "a/../b", "./a", "a\\b", "C:/x", "a\0b",
    ] {
        assert_eq!(
            validate_rel_path(bad),
            Err(ProtoError::InvalidPath),
            "{bad:?}"
        );
    }
}

#[test]
fn fetch_frames() {
    let id = Uuid::from_u128(3);
    let req = FetchReq::new(id, FetchTarget::File(2), CHUNK_GROUP * 3);
    assert_eq!(req.validate(), Ok(()));
    let (b, _): (FetchReq, usize) = decode_frame(&encode_frame(&req).unwrap()).unwrap();
    assert_eq!(b, req);
    assert_eq!(
        FetchReq::new(id, FetchTarget::Rep(0), 1).validate(),
        Err(ProtoError::MisalignedOffset)
    );
    let mut v2 = req;
    v2.v = 2;
    assert_eq!(v2.validate(), Err(ProtoError::UnsupportedVersion(2)));
    for status in [
        FetchStatus::Ok,
        FetchStatus::NotFound,
        FetchStatus::Changed,
        FetchStatus::Denied,
    ] {
        let r = FetchResp {
            status,
            size: 9,
            hash: [1; 32],
        };
        let (b, _): (FetchResp, usize) = decode_frame(&encode_frame(&r).unwrap()).unwrap();
        assert_eq!(b, r);
    }
}
