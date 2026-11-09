//! Who made a clipboard change. X11 resolves the selection owner window to a process
//! (XRes client PID, then `_NET_WM_PID`) and reads WM_CLASS; Wayland data-control has no source
//! identity, so the focused window from the compositor IPC is used (best effort, see
//! `source_app_ipc`). `/proc` lookups only work for processes in our PID namespace.

use super::{BackendKind, SourceApp};

/// `/proc/<pid>/comm`, else the basename of `/proc/<pid>/exe`.
pub fn proc_name(pid: u32) -> Option<String> {
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    if comm.is_some() {
        return comm;
    }
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    exe.file_name().map(|n| n.to_string_lossy().into_owned())
}

/// A `SourceApp` for a known PID plus window classes (WM_CLASS or app id).
pub fn for_pid(pid: Option<u32>, classes: Vec<String>) -> SourceApp {
    SourceApp {
        pid,
        name: pid.and_then(proc_name),
        classes,
    }
}

/// WM_CLASS is "instance\0class\0".
pub fn parse_wm_class(raw: &[u8]) -> Vec<String> {
    raw.split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect()
}

/// One line for the status page: what the sensitive filter can see on this session.
pub fn capability(kind: BackendKind, env: &dyn Fn(&str) -> Option<String>) -> &'static str {
    match kind {
        BackendKind::X11 => "Password hints and the copying app (X11).",
        BackendKind::Wayland => {
            if env("HYPRLAND_INSTANCE_SIGNATURE").is_some() || env("SWAYSOCK").is_some() {
                "Password hints and the focused app (best effort)."
            } else {
                "Password hints only: other password managers may be synced."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wm_class_splits_instance_and_class() {
        assert_eq!(
            parse_wm_class(b"keepassxc\0KeePassXC\0"),
            ["keepassxc", "KeePassXC"]
        );
        assert!(parse_wm_class(b"").is_empty());
    }

    #[test]
    fn own_process_resolves_and_garbage_pid_does_not() {
        assert!(proc_name(std::process::id()).is_some());
        assert!(proc_name(u32::MAX).is_none());
    }

    #[test]
    fn capability_names_what_the_filter_can_see() {
        let none = |_: &str| None;
        let sway = |k: &str| (k == "SWAYSOCK").then(|| "/x".to_owned());
        assert!(capability(BackendKind::X11, &none).contains("copying app"));
        assert!(capability(BackendKind::Wayland, &sway).contains("focused app"));
        assert!(capability(BackendKind::Wayland, &none).contains("hints only"));
    }
}
