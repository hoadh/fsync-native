//! Which clipboard backend a session gets, and why. `FSYNC_CLIPBOARD_BACKEND=wayland|x11|manual`
//! overrides detection (also the rollback switch: `manual` disables auto sync without a
//! rebuild). The reason is shown on the status page.

use super::wayland_backend::{WaylandBackend, probe as wayland_probe};
use super::x11_backend::{X11Backend, probe as x11_probe};
use super::{Backend, BackendKind};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    Auto(BackendKind),
    /// No background clipboard access: sending and pasting go through the Send/Paste window.
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detection {
    pub choice: Choice,
    pub reason: String,
}

fn det(choice: Choice, reason: impl Into<String>) -> Detection {
    Detection {
        choice,
        reason: reason.into(),
    }
}

/// Pure decision; the probes say whether Wayland data-control / an X server are usable.
pub fn decide(
    env: &dyn Fn(&str) -> Option<String>,
    wayland: &dyn Fn() -> Result<(), String>,
    x11: &dyn Fn() -> Result<(), String>,
) -> Detection {
    let set = |k: &str| env(k).is_some_and(|v| !v.is_empty());
    if let Some(forced) = env("FSYNC_CLIPBOARD_BACKEND") {
        return match forced.as_str() {
            "manual" => det(Choice::Manual, "FSYNC_CLIPBOARD_BACKEND=manual"),
            "wayland" => match wayland() {
                Ok(()) => det(
                    Choice::Auto(BackendKind::Wayland),
                    "FSYNC_CLIPBOARD_BACKEND=wayland",
                ),
                Err(e) => det(
                    Choice::Manual,
                    format!("FSYNC_CLIPBOARD_BACKEND=wayland unusable: {e}"),
                ),
            },
            "x11" => match x11() {
                Ok(()) => det(
                    Choice::Auto(BackendKind::X11),
                    "FSYNC_CLIPBOARD_BACKEND=x11",
                ),
                Err(e) => det(
                    Choice::Manual,
                    format!("FSYNC_CLIPBOARD_BACKEND=x11 unusable: {e}"),
                ),
            },
            other => det(
                Choice::Manual,
                format!("unknown FSYNC_CLIPBOARD_BACKEND '{other}'"),
            ),
        };
    }
    if set("WAYLAND_DISPLAY") {
        return match wayland() {
            Ok(()) => det(
                Choice::Auto(BackendKind::Wayland),
                "Wayland compositor with data-control",
            ),
            Err(e) => det(
                Choice::Manual,
                format!("{e} (GNOME has none): use the Send and Paste commands"),
            ),
        };
    }
    if set("DISPLAY") {
        return match x11() {
            Ok(()) => det(Choice::Auto(BackendKind::X11), "X11 session"),
            Err(e) => det(Choice::Manual, e),
        };
    }
    det(Choice::Manual, "no display server found")
}

pub fn detect(env: &dyn Fn(&str) -> Option<String>) -> Detection {
    decide(env, &|| wayland_probe().map(|_| ()), &x11_probe)
}

/// Starts the backend a detection chose; `None` for Manual.
pub fn start(choice: &Choice) -> Result<Option<Arc<dyn Backend>>, String> {
    Ok(match choice {
        Choice::Manual => None,
        Choice::Auto(BackendKind::Wayland) => Some(WaylandBackend::start()? as Arc<dyn Backend>),
        Choice::Auto(BackendKind::X11) => Some(X11Backend::start(None)? as Arc<dyn Backend>),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_owned())
        }
    }
    const OK: fn() -> Result<(), String> = || Ok(());
    const NO: fn() -> Result<(), String> = || Err("no data-control".into());

    #[test]
    fn wayland_with_data_control_is_auto() {
        let d = decide(
            &env(&[("WAYLAND_DISPLAY", "w-1"), ("DISPLAY", ":0")]),
            &OK,
            &OK,
        );
        assert_eq!(d.choice, Choice::Auto(BackendKind::Wayland));
    }

    #[test]
    fn gnome_style_wayland_is_manual_even_with_xwayland() {
        let d = decide(
            &env(&[("WAYLAND_DISPLAY", "w-1"), ("DISPLAY", ":0")]),
            &NO,
            &OK,
        );
        assert_eq!(d.choice, Choice::Manual);
        assert!(d.reason.contains("GNOME"));
    }

    #[test]
    fn x11_only_and_nothing() {
        assert_eq!(
            decide(&env(&[("DISPLAY", ":0")]), &NO, &OK).choice,
            Choice::Auto(BackendKind::X11)
        );
        assert_eq!(decide(&env(&[]), &OK, &OK).choice, Choice::Manual);
        assert_eq!(
            decide(&env(&[("DISPLAY", ":0")]), &NO, &NO).choice,
            Choice::Manual
        );
    }

    #[test]
    fn override_wins_and_fails_safe() {
        let manual = env(&[
            ("FSYNC_CLIPBOARD_BACKEND", "manual"),
            ("WAYLAND_DISPLAY", "w"),
        ]);
        assert_eq!(decide(&manual, &OK, &OK).choice, Choice::Manual);
        let x11 = env(&[
            ("FSYNC_CLIPBOARD_BACKEND", "x11"),
            ("WAYLAND_DISPLAY", "w"),
            ("DISPLAY", ":1"),
        ]);
        assert_eq!(
            decide(&x11, &OK, &OK).choice,
            Choice::Auto(BackendKind::X11)
        );
        let broken = env(&[("FSYNC_CLIPBOARD_BACKEND", "wayland")]);
        assert_eq!(decide(&broken, &NO, &OK).choice, Choice::Manual);
        let junk = env(&[("FSYNC_CLIPBOARD_BACKEND", "gtk")]);
        assert_eq!(decide(&junk, &OK, &OK).choice, Choice::Manual);
    }
}
