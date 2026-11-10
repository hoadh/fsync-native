use super::*;

#[test]
fn hyprland_activewindow_json() {
    let json = r#"{"address":"0x1","class":"org.keepassxc.KeePassXC","title":"x",
                   "initialClass":"org.keepassxc.KeePassXC","pid":4242,"xwayland":false}"#;
    let app = parse_hyprland_activewindow(json).expect("app");
    assert_eq!(app.pid, Some(4242));
    assert_eq!(app.classes, ["org.keepassxc.KeePassXC"]);
    assert!(parse_hyprland_activewindow("{}").is_none());
    assert!(parse_hyprland_activewindow("not json").is_none());
}

#[test]
fn sway_tree_finds_the_focused_leaf() {
    let json = r#"{"focused":false,"nodes":[{"focused":false,"nodes":[
        {"focused":false,"app_id":"foot","pid":10,"nodes":[]},
        {"focused":true,"app_id":"Bitwarden","pid":11,"nodes":[]}],"floating_nodes":[]}]}"#;
    let app = parse_sway_tree(json).expect("app");
    assert_eq!(app.pid, Some(11));
    assert_eq!(app.classes, ["Bitwarden"]);
}

#[test]
fn sway_tree_reads_xwayland_class_and_handles_no_focus() {
    let x = r#"{"nodes":[{"focused":true,"app_id":null,"pid":7,
                "window_properties":{"class":"KeePassXC","instance":"keepassxc"},"nodes":[]}]}"#;
    let app = parse_sway_tree(x).expect("app");
    assert_eq!(app.classes, ["KeePassXC", "keepassxc"]);
    assert!(parse_sway_tree(r#"{"focused":false,"nodes":[]}"#).is_none());
}

#[test]
fn unknown_compositor_and_bad_hypr_signature_give_none() {
    let none = |_: &str| None;
    assert!(focused_app(&FocusEnv::capture(&none)).is_none());
    let bad = |k: &str| match k {
        "HYPRLAND_INSTANCE_SIGNATURE" => Some("../evil".to_owned()),
        "XDG_RUNTIME_DIR" => Some("/tmp".to_owned()),
        _ => None,
    };
    assert!(focused_app(&FocusEnv::capture(&bad)).is_none());
}
