//! Password managers that are always skipped (names compared case-insensitively against the
//! process name, WM_CLASS and Wayland app id). The user's list from core settings is added on
//! top. Ported from the Windows and macOS apps plus the Linux managers.

pub const BUILTIN_DENYLIST: &[&str] = &[
    // Windows / macOS lists (names without ".exe" / bundle ids).
    "1password",
    "com.1password.1password",
    "keepass",
    "keepassxc",
    "bitwarden",
    "dashlane",
    "lastpass",
    "enpass",
    "nordpass",
    "protonpass",
    "proton pass",
    "com.bitwarden.desktop",
    // Linux
    "keepass2",
    "org.keepassxc.keepassxc",
    "kwalletmanager5",
    "kwalletmanager6",
    "org.kde.kwalletmanager",
    "seahorse",
    "org.gnome.seahorse.application",
    "gnome-passwordsafe",
    "secrets",
    "org.gnome.world.secrets",
    "gopass",
];

/// Lowercased, trimmed, without a trailing ".exe" and without a directory part.
pub fn normalize(name: &str) -> String {
    let base = name.trim().rsplit('/').next().unwrap_or("").to_lowercase();
    base.strip_suffix(".exe").map(str::to_owned).unwrap_or(base)
}

/// Built-in list plus the user's entries, normalized and de-duplicated.
pub fn effective(user: &[String]) -> Vec<String> {
    let mut out: Vec<String> = BUILTIN_DENYLIST.iter().map(|s| (*s).to_owned()).collect();
    for u in user {
        let n = normalize(u);
        if !n.is_empty() && !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_case_path_and_exe() {
        assert_eq!(normalize(" KeePassXC.EXE "), "keepassxc");
        assert_eq!(normalize("/usr/bin/Bitwarden"), "bitwarden");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn user_entries_are_added_once() {
        let l = effective(&["MyVault".into(), "keepassxc".into(), "  ".into()]);
        assert!(l.contains(&"myvault".to_owned()));
        assert_eq!(l.iter().filter(|e| *e == "keepassxc").count(), 1);
        assert!(!l.contains(&String::new()));
    }

    #[test]
    fn linux_managers_are_built_in() {
        for n in [
            "org.keepassxc.keepassxc",
            "seahorse",
            "kwalletmanager6",
            "enpass",
        ] {
            assert!(BUILTIN_DENYLIST.contains(&n), "{n}");
        }
    }
}
