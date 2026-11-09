//! Sensitive-content filters. Decided on names only, before any bytes are read (ADR 013
//! decision 4): own marker, privacy hints, then the source application against the built-in and
//! the user's denylist. An unknown source is allowed (accepted limit, shown in the first-run
//! notice).

use super::denylist_builtin::normalize;
use super::echo::is_own;
use super::{CONCEALED_MIME, KDE_HINT_MIME, SourceApp};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Skip {
    OwnEcho,
    /// A privacy hint is offered (password manager, concealed content).
    Hinted,
    /// The source application is on the denylist (the name is not logged or shown).
    DeniedApp,
    NoContent,
    TooLarge,
    BadImage,
    Unreadable,
    /// Files: every path was refused (protected, missing or remote).
    NoUsableFiles,
}

impl Skip {
    /// Text for the user when they asked to send explicitly (also shown for automatic skips that
    /// the user can act on, see `sync::shared`).
    pub fn user_text(&self) -> Option<&'static str> {
        match self {
            Self::OwnEcho => Some("Not sent: that is what fsync just received."),
            Self::Hinted => Some("Not sent: it looks like a password or other secret."),
            Self::DeniedApp => Some("Not sent: this app is on your privacy list."),
            Self::NoContent => Some("Not sent: nothing to sync (text, rich text, image or files)."),
            Self::TooLarge => Some("Not sent: larger than the clip size limit."),
            Self::BadImage => Some("Not sent: the image is too large or not a PNG."),
            Self::Unreadable => Some("Not sent: the clipboard owner did not answer in time."),
            Self::NoUsableFiles => Some("Not sent: the copied files are protected or not local."),
        }
    }
}

/// Own entry first (received clips carry the hint too), then privacy hints.
pub fn classify_mimes(mimes: &[String]) -> Result<(), Skip> {
    if is_own(mimes) {
        return Err(Skip::OwnEcho);
    }
    // MIME names are case-insensitive: a hint in another case must still count.
    if mimes
        .iter()
        .any(|m| m.eq_ignore_ascii_case(KDE_HINT_MIME) || m.eq_ignore_ascii_case(CONCEALED_MIME))
    {
        return Err(Skip::Hinted);
    }
    Ok(())
}

/// `denylist`: `denylist_builtin::effective(user)` (already normalized).
pub fn classify_source(source: Option<&SourceApp>, denylist: &[String]) -> Result<(), Skip> {
    let Some(app) = source else {
        return Ok(());
    };
    let denied = app
        .name
        .iter()
        .chain(app.classes.iter())
        .map(|n| normalize(n))
        .filter(|n| !n.is_empty())
        .any(|n| denylist.contains(&n));
    if denied { Err(Skip::DeniedApp) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::MARKER_MIME;
    use crate::clipboard::denylist_builtin::effective;

    fn m(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn own_marker_wins_over_the_hint() {
        let r = classify_mimes(&m(&["text/plain", MARKER_MIME, KDE_HINT_MIME]));
        assert_eq!(r, Err(Skip::OwnEcho));
    }

    #[test]
    fn hints_skip_and_plain_content_passes() {
        assert_eq!(
            classify_mimes(&m(&["text/plain", KDE_HINT_MIME])),
            Err(Skip::Hinted)
        );
        assert_eq!(
            classify_mimes(&m(&["text/plain", CONCEALED_MIME])),
            Err(Skip::Hinted)
        );
        assert_eq!(classify_mimes(&m(&["text/plain", "text/html"])), Ok(()));
    }

    #[test]
    fn denylist_matches_name_class_and_app_id_case_insensitively() {
        let list = effective(&["MyVault".into()]);
        let by_comm = SourceApp {
            name: Some("keepassxc".into()),
            ..Default::default()
        };
        let by_class = SourceApp {
            classes: vec!["keepassxc".into(), "KeePassXC".into()],
            ..Default::default()
        };
        let by_app_id = SourceApp {
            classes: vec!["org.keepassxc.KeePassXC".into()],
            ..Default::default()
        };
        let by_user = SourceApp {
            name: Some("MYVAULT".into()),
            ..Default::default()
        };
        for app in [by_comm, by_class, by_app_id, by_user] {
            assert_eq!(
                classify_source(Some(&app), &list),
                Err(Skip::DeniedApp),
                "{app:?}"
            );
        }
    }

    #[test]
    fn unknown_or_ordinary_sources_are_allowed() {
        let list = effective(&[]);
        assert_eq!(classify_source(None, &list), Ok(()));
        let firefox = SourceApp {
            name: Some("firefox".into()),
            classes: vec!["firefox".into()],
            pid: Some(1),
        };
        assert_eq!(classify_source(Some(&firefox), &list), Ok(()));
        // Substrings do not match: only exact names.
        let near = SourceApp {
            name: Some("keepassx-helper".into()),
            ..Default::default()
        };
        assert_eq!(classify_source(Some(&near), &list), Ok(()));
    }

    #[test]
    fn explicit_send_always_gets_a_reason() {
        for s in [
            Skip::OwnEcho,
            Skip::Hinted,
            Skip::DeniedApp,
            Skip::NoContent,
        ] {
            assert!(s.user_text().is_some(), "{s:?}");
        }
    }
}
