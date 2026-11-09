#![cfg(not(feature = "e2e-control"))]
use fsync_linux_platform::ipc::IpcRequest;

#[test]
fn e2e_commands_do_not_parse_without_the_feature() {
    for c in [
        "status",
        "login",
        "pair-offer",
        "pair-join",
        "pair-confirm",
        "cancel",
        "clip-mode",
        "offer-accept",
        "history-copy",
        "history-download",
    ] {
        assert!(
            IpcRequest::parse_line(&format!(r#"{{"cmd":"{c}","email":"a"}}"#)).is_err(),
            "{c}"
        );
    }
}

#[test]
fn activation_token_comes_from_env_in_order() {
    use fsync_linux_platform::ipc::IpcCommand;
    let env = |k: &str| (k == "DESKTOP_STARTUP_ID").then(|| "sid".to_string());
    let r = IpcRequest::new(IpcCommand::Show).with_activation_from_env(&env);
    assert_eq!(r.activation_token.as_deref(), Some("sid"));
    let env2 = |k: &str| Some(format!("{k}-v"));
    let r = IpcRequest::new(IpcCommand::Show).with_activation_from_env(&env2);
    assert_eq!(
        r.activation_token.as_deref(),
        Some("XDG_ACTIVATION_TOKEN-v")
    );
}
