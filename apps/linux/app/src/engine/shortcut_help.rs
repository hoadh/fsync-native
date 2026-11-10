//! Hotkey bind lines per desktop: one source for the Settings screen (`SettingsObject.shortcuts`),
//! `fsync-linux --help` and the README. Hotkeys are plain commands that reach the running app
//! over its IPC socket; the app never grabs keys itself. Defaults are Super+Alt+C / Super+Alt+V:
//! never Ctrl+Alt (AltGr on many layouts, ADR 013 d8).

pub struct Bind {
    pub desktop: &'static str,
    /// What to do, shown above the lines.
    pub how: &'static str,
    /// Copyable lines (config text or the command).
    pub lines: &'static [&'static str],
}

pub const BINDS: &[Bind] = &[
    Bind {
        desktop: "Hyprland (Omarchy)",
        how: "Add to ~/.config/hypr/bindings.conf (or hyprland.conf):",
        lines: &[
            "bind = SUPER ALT, C, exec, fsync-linux --clip-send",
            "bind = SUPER ALT, V, exec, fsync-linux --clip-paste",
        ],
    },
    Bind {
        desktop: "Sway / i3-style",
        how: "Add to ~/.config/sway/config:",
        lines: &[
            "bindsym Mod4+Mod1+c exec fsync-linux --clip-send",
            "bindsym Mod4+Mod1+v exec fsync-linux --clip-paste",
        ],
    },
    Bind {
        desktop: "KDE Plasma",
        how: "System Settings > Keyboard > Shortcuts > Add Command, then set the key:",
        lines: &["fsync-linux --clip-send", "fsync-linux --clip-paste"],
    },
    Bind {
        desktop: "GNOME",
        how: "Settings > Keyboard > Custom Shortcuts: a small window opens to confirm the action. Commands:",
        lines: &["fsync-linux --clip-send", "fsync-linux --clip-paste"],
    },
];

/// `fsync-linux --help`.
pub fn help_text() -> String {
    let mut out = String::from(
        "fsync-linux [--background] [--data-dir DIR] [--show] [--clip-send] [--clip-paste]\n\
         \x20           [--send PATH...] [--version] [--build-info]\n\n\
         --clip-send / --clip-paste act in the running instance, so they work as hotkey commands.\n\
         --send opens a device picker for the given files (home folder and removable media only).\n\n\
         Hotkey binds (Super+Alt; never Ctrl+Alt, it is AltGr on many layouts):\n",
    );
    for b in BINDS {
        out.push_str(&format!("\n{}\n  {}\n", b.desktop, b.how));
        for l in b.lines {
            out.push_str(&format!("    {l}\n"));
        }
    }
    out
}

/// JSON for QML: `[{"desktop","how","lines":[...]}]`.
pub fn json() -> String {
    let items: Vec<_> = BINDS
        .iter()
        .map(|b| serde_json::json!({"desktop": b.desktop, "how": b.how, "lines": b.lines}))
        .collect();
    serde_json::Value::Array(items).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_never_use_ctrl_alt_and_name_the_real_flags() {
        for b in BINDS {
            for l in b.lines {
                let low = l.to_lowercase();
                assert!(
                    !low.contains("ctrl alt") && !low.contains("control+alt"),
                    "{l}"
                );
                assert!(
                    !low.contains("ctrl+alt") && !low.contains("ctrl, alt"),
                    "{l}"
                );
            }
        }
        let t = help_text();
        assert!(t.contains("--clip-send") && t.contains("--clip-paste"));
        let v: serde_json::Value = serde_json::from_str(&json()).unwrap();
        assert_eq!(v.as_array().unwrap().len(), BINDS.len());
    }
}
