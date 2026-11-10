#![cfg(feature = "e2e-control")]
use fsync_linux_platform::ipc::{E2eCommand, IpcCommand, IpcRequest};

#[test]
fn e2e_commands_parse_and_roundtrip() {
    let r = IpcRequest::parse_line(r#"{"cmd":"login","email":"a@b.c","code":"123456"}"#).unwrap();
    assert_eq!(
        r.command,
        IpcCommand::E2e(E2eCommand::Login {
            email: "a@b.c".into(),
            code: Some("123456".into())
        })
    );
    let c = IpcCommand::E2e(E2eCommand::PairConfirm {
        request_id: "7".into(),
        yes: true,
    });
    let back = IpcRequest::parse_line(&IpcRequest::new(c.clone()).to_line().unwrap()).unwrap();
    assert_eq!(back.command, c);
    assert!(IpcRequest::parse_line(r#"{"cmd":"pair-join"}"#).is_err());
    for c in ["status", "pair-offer", "cancel"] {
        assert!(IpcRequest::parse_line(&format!(r#"{{"cmd":"{c}"}}"#)).is_ok());
    }
}

#[test]
fn offer_and_history_commands_roundtrip() {
    for c in [
        E2eCommand::OfferAccept { item_id: None },
        E2eCommand::OfferAccept {
            item_id: Some("0b1c".into()),
        },
        E2eCommand::HistoryCopy {
            item_id: "0b1c".into(),
        },
        E2eCommand::HistoryDownload {
            item_id: "0b1c".into(),
        },
        E2eCommand::PickDevice {
            name: "devB".into(),
        },
    ] {
        let c = IpcCommand::E2e(c);
        let back = IpcRequest::parse_line(&IpcRequest::new(c.clone()).to_line().unwrap()).unwrap();
        assert_eq!(back.command, c);
    }
    assert!(IpcRequest::parse_line(r#"{"cmd":"history-copy"}"#).is_err());
    assert!(IpcRequest::parse_line(r#"{"cmd":"pick-device"}"#).is_err());
    assert!(IpcRequest::parse_line(r#"{"cmd":"offer-accept"}"#).is_ok());
}
