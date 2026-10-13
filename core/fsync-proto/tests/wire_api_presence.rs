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
fn api_dtos_json() {
    let g = genesis(&key(1));
    let resp = DeviceListResponse::new(&g).unwrap();
    assert_eq!(resp.seq, 0);
    let json = serde_json::to_string(&resp).unwrap();
    let back: DeviceListResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back.decode().unwrap(), g);
    assert_eq!(PutDeviceList::new(&g).unwrap().decode().unwrap(), g);
    assert_eq!(decode_signed("***"), Err(ProtoError::InvalidBase64));
    assert!(decode_signed("AAAA").is_err());

    let v = OtpVerifyResponse {
        tokens: TokenPair {
            access: "a".into(),
            refresh: "r".into(),
        },
        account_id: acct(),
    };
    let j = serde_json::to_value(&v).unwrap();
    assert_eq!(j["access"], "a");
    assert_eq!(j["refresh"], "r");
    assert_eq!(serde_json::from_value::<OtpVerifyResponse>(j).unwrap(), v);
    assert!(
        !format!("{:?}", v.tokens).contains('a') || format!("{:?}", v.tokens).contains("redacted")
    );
    assert_eq!(
        format!(
            "{:?}",
            Refresh {
                refresh: "secret".into()
            }
        ),
        "Refresh(<redacted>)"
    );

    let c = PairingCreate {
        node_id: nid(&key(1)),
        relay_url: None,
    };
    assert_eq!(
        serde_json::from_str::<PairingCreate>(&serde_json::to_string(&c).unwrap()).unwrap(),
        c
    );
    let l = PairingLookup {
        node_id: nid(&key(1)),
        relay_url: Some("https://r".into()),
    };
    assert_eq!(
        serde_json::from_str::<PairingLookup>(&serde_json::to_string(&l).unwrap()).unwrap(),
        l
    );
    let created = PairingCreated {
        code: "123456".into(),
        expires_at: 5,
    };
    assert_eq!(serde_json::to_value(&created).unwrap()["expires_at"], 5);
    assert!(is_valid_pairing_code("012345"));
    for bad in ["12345", "1234567", "12345a", "١٢٣٤٥٦"] {
        assert!(!is_valid_pairing_code(bad), "{bad}");
    }
    let _ = (
        OtpStart {
            email: "a@b".into(),
        },
        OtpVerify {
            email: "a@b".into(),
            code: "1".into(),
        },
    );
    let _ = ApiErrorBody {
        code: "x".into(),
        message: "y".into(),
    };
}

#[test]
fn presence_messages_json() {
    let k = key(1);
    let nonce = Nonce::from([5; 32]);
    let sig = sign_challenge(&k, &nonce);
    assert_eq!(verify_challenge(&nid(&k), &nonce, &sig), Ok(()));
    assert_eq!(
        verify_challenge(&nid(&key(2)), &nonce, &sig),
        Err(ProtoError::BadSignature)
    );
    assert_eq!(
        verify_challenge(&nid(&k), &Nonce::from([6; 32]), &sig),
        Err(ProtoError::BadSignature)
    );

    let auth = ClientMsg::Auth {
        access: "tok".into(),
        node_id: nid(&k),
        sig,
    };
    let j = serde_json::to_value(&auth).unwrap();
    assert_eq!(j["type"], "auth");
    assert_eq!(serde_json::from_value::<ClientMsg>(j).unwrap(), auth);
    assert!(!format!("{auth:?}").contains("tok"));

    let addrs = Addrs {
        relay_url: Some("https://r".into()),
        direct: vec![
            "192.168.1.2:4000".parse().unwrap(),
            "[::1]:1".parse().unwrap(),
        ],
    };
    for m in [
        ClientMsg::Hello {
            addrs: addrs.clone(),
        },
        ClientMsg::Heartbeat,
    ] {
        assert_eq!(
            serde_json::from_str::<ClientMsg>(&serde_json::to_string(&m).unwrap()).unwrap(),
            m
        );
        let _ = format!("{m:?}");
    }
    assert_eq!(
        serde_json::to_value(ClientMsg::Heartbeat).unwrap()["type"],
        "heartbeat"
    );
    let peer = PeerInfo {
        node_id: nid(&k),
        online: true,
        addrs,
        last_seen_ms: 9,
    };
    for m in [
        ServerMsg::Challenge { nonce },
        ServerMsg::Peers {
            peers: vec![peer.clone()],
        },
        ServerMsg::PeerUpdate(peer),
        ServerMsg::DeviceListChanged { seq: 4 },
    ] {
        assert_eq!(
            serde_json::from_str::<ServerMsg>(&serde_json::to_string(&m).unwrap()).unwrap(),
            m
        );
    }
    assert!(serde_json::from_str::<ClientMsg>(r#"{"type":"nope"}"#).is_err());
    // wrong-length nonce
    assert!(serde_json::from_str::<ServerMsg>(r#"{"type":"challenge","nonce":"AAAA"}"#).is_err());
}
