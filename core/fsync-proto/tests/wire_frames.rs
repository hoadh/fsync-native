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
fn alpns() {
    assert_eq!(ALPN_CLIP, b"fsync/clip/1");
    assert_eq!(ALPN_FILE, b"fsync/file/1");
    assert_eq!(ALPN_PAIR, b"fsync/pair/1");
}

#[test]
fn node_id_formats() {
    let n = nid(&key(1));
    let s = n.to_string();
    assert_eq!(s.len(), 64);
    assert_eq!(s.parse::<NodeId>().unwrap(), n);
    assert_eq!(s.to_uppercase().parse::<NodeId>().unwrap(), n);
    assert_eq!(NodeId::from_base32(&n.to_base32()).unwrap(), n);
    assert_eq!("zz".parse::<NodeId>(), Err(ProtoError::InvalidNodeId));
    assert_eq!("ab".parse::<NodeId>(), Err(ProtoError::InvalidNodeId));
    assert_eq!(NodeId::from_base32("!!"), Err(ProtoError::InvalidNodeId));
    assert_eq!(NodeId::from_base32("aa"), Err(ProtoError::InvalidNodeId));
    assert!(n.verifying_key().is_ok());
    assert_eq!(serde_json::to_string(&n).unwrap(), format!("\"{s}\""));
    assert_eq!(
        serde_json::from_str::<NodeId>(&format!("\"{s}\"")).unwrap(),
        n
    );
    assert!(serde_json::from_str::<NodeId>("\"nope\"").is_err());
    assert_eq!(postcard::to_stdvec(&n).unwrap().len(), 32);
    assert!(format!("{n:?}").starts_with("NodeId("));
    assert_eq!(*n.as_bytes(), n.0);
}

#[test]
fn frame_roundtrip_and_errors() {
    let o = offer();
    let f = encode_frame(&o).unwrap();
    let (back, used): (ClipOffer, usize) = decode_frame(&f).unwrap();
    assert_eq!((back, used), (o.clone(), f.len()));
    assert_eq!(
        payload_len(f[..4].try_into().unwrap()).unwrap(),
        f.len() - 4
    );
    // truncated header / body
    assert_eq!(
        decode_frame::<ClipOffer>(&f[..2]),
        Err(ProtoError::Truncated { needed: 2 })
    );
    assert_eq!(
        decode_frame::<ClipOffer>(&f[..f.len() - 3]),
        Err(ProtoError::Truncated { needed: 3 })
    );
    // oversized header rejected before allocation
    let huge = ((MAX_FRAME_LEN + 1) as u32).to_le_bytes();
    assert!(matches!(
        decode_frame::<ClipOffer>(&huge),
        Err(ProtoError::FrameTooLarge { .. })
    ));
    // oversized encode
    let big = vec![0u8; MAX_FRAME_LEN + 1];
    assert!(matches!(
        encode_frame(&big),
        Err(ProtoError::FrameTooLarge { .. })
    ));
    // trailing bytes inside a frame
    let mut p = postcard::to_stdvec(&1u8).unwrap();
    p.push(0);
    assert_eq!(decode_payload::<u8>(&p), Err(ProtoError::TrailingBytes));
    // two frames back to back
    let two = [encode_frame(&1u8).unwrap(), encode_frame(&2u8).unwrap()].concat();
    let (a, n) = decode_frame::<u8>(&two).unwrap();
    assert_eq!((a, decode_frame::<u8>(&two[n..]).unwrap().0), (1, 2));
}

#[test]
fn pair_frames_roundtrip() {
    let g = genesis(&key(1));
    let frames = vec![
        PairFrame::Hello {
            v: 1,
            mode: PairMode::Code,
            node_id: nid(&key(1)),
            nonce: [7; 16],
        },
        PairFrame::Hello {
            v: 1,
            mode: PairMode::Qr,
            node_id: nid(&key(1)),
            nonce: [8; 16],
        },
        PairFrame::HelloReply { nonce: [9; 16] },
        PairFrame::Accept,
        PairFrame::Spake { msg: vec![1, 2, 3] },
        PairFrame::Confirm { mac: [4; 32] },
        PairFrame::DeviceInfo {
            name: "w".into(),
            platform: Platform::Win,
        },
        PairFrame::SignedList {
            chain: vec![g.clone(), g],
        },
        PairFrame::Abort {
            reason: AbortReason::BadMac,
        },
        PairFrame::Done,
    ];
    for f in frames {
        let (back, _): (PairFrame, usize) = decode_frame(&encode_frame(&f).unwrap()).unwrap();
        assert_eq!(back, f);
    }
}

#[test]
fn qr_roundtrip_and_errors() {
    let q = QrPayload {
        node_id: nid(&key(1)),
        secret: [7; 16],
        relay: Some("https://relay.example/a?b=c&d".into()),
    };
    let s = q.build();
    assert!(s.starts_with("fsync://pair?v=1&n="));
    assert_eq!(QrPayload::parse(&s).unwrap(), q);
    let no_relay = QrPayload {
        node_id: q.node_id,
        secret: [1; 16],
        relay: None,
    };
    assert_eq!(QrPayload::parse(&no_relay.build()).unwrap(), no_relay);
    assert!(!format!("{q:?}").contains("AAAA") && format!("{q:?}").contains("redacted"));

    let n = q.node_id.to_base32();
    let sec = "BwcHBwcHBwcHBwcHBwcHBw";
    let bad = |u: String| matches!(QrPayload::parse(&u), Err(ProtoError::InvalidQr(_)));
    assert!(bad("not a url".into()));
    assert!(bad(format!("http://pair?v=1&n={n}&s={sec}")));
    assert!(bad(format!("fsync://other?v=1&n={n}&s={sec}")));
    assert!(bad(format!("fsync://pair?n={n}&s={sec}")));
    assert!(bad(format!("fsync://pair?v=2&n={n}&s={sec}")));
    assert!(bad(format!("fsync://pair?v=1&s={sec}")));
    assert!(bad(format!("fsync://pair?v=1&n=!!&s={sec}")));
    assert!(bad(format!("fsync://pair?v=1&n={n}")));
    assert!(bad(format!("fsync://pair?v=1&n={n}&s=***")));
    assert!(bad(format!("fsync://pair?v=1&n={n}&s=AAAA")));
    assert!(bad(format!("fsync://pair?v=1&n={n}&s={sec}&s={sec}")));
    assert!(QrPayload::parse(&format!("fsync://pair?v=1&n={n}&s={sec}&x=1")).is_ok());
}
