//! The Settings screen's data: core `Settings` as a form (sizes in MiB, denylist as lines) and
//! the save with its side effects. Core validates; this only maps and re-applies what a change
//! affects (clip mode in the engine status, inbox retention).

use super::{clipboard_runtime, inbox_runtime, user_messages};
use crate::core_state::Core;
use fsync_core::service::{ClipMode, Settings};
use std::sync::Arc;

const MIB: u64 = 1 << 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    pub manual: bool,
    pub retention_days: u32,
    pub retention_mib: u64,
    pub max_clip_mib: u64,
    pub auto_accept: bool,
    /// One entry per line.
    pub denylist: String,
}

impl Form {
    pub fn of(s: &Settings) -> Self {
        Self {
            manual: s.clip_mode == ClipMode::Manual,
            retention_days: s.retention_days,
            retention_mib: s.retention_bytes.div_ceil(MIB),
            max_clip_mib: s.max_clip_bytes.div_ceil(MIB),
            auto_accept: s.auto_accept_files,
            denylist: s.denylist.join("\n"),
        }
    }
}

fn parse_denylist(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if !out.iter().any(|o| o == line) {
            out.push(line.to_owned());
        }
    }
    out
}

/// `cur` with the form applied. A size shown rounded up to MiB is written back only when the
/// user changed it, so an untouched byte value (set elsewhere) is never altered by a save.
pub fn apply(cur: &Settings, f: &Form) -> Settings {
    let mut s = cur.clone();
    s.clip_mode = if f.manual {
        ClipMode::Manual
    } else {
        ClipMode::Auto
    };
    s.retention_days = f.retention_days;
    if f.retention_mib != cur.retention_bytes.div_ceil(MIB) {
        s.retention_bytes = f.retention_mib.saturating_mul(MIB);
    }
    if f.max_clip_mib != cur.max_clip_bytes.div_ceil(MIB) {
        s.max_clip_bytes = f.max_clip_mib.saturating_mul(MIB);
    }
    s.auto_accept_files = f.auto_accept;
    s.denylist = parse_denylist(&f.denylist);
    s
}

/// MiB for the byte value the form shows as `shown` (a SpinBox holds an `i32`): the loaded
/// value itself while the user left the field alone, so a size the box cannot show (or that
/// is not a whole MiB) is never rewritten by a save of another field.
pub fn resolve_mib(shown: i64, baseline_mib: u64) -> u64 {
    let displayed = i32::try_from(baseline_mib).map_or(i64::from(i32::MAX), i64::from);
    if shown == displayed {
        baseline_mib
    } else {
        u64::try_from(shown).unwrap_or(0)
    }
}

/// A retention change affects the inbox: sweep now instead of at the next daily pass.
pub fn needs_retention_sweep(cur: &Settings, next: &Settings) -> bool {
    next.retention_days != cur.retention_days || next.retention_bytes != cur.retention_bytes
}

pub fn clip_mode_changed(cur: &Settings, next: &Settings) -> bool {
    next.clip_mode != cur.clip_mode
}

pub fn load(core: &Core) -> Result<Form, String> {
    let svc = core.service().ok_or("The engine is not ready yet.")?;
    svc.settings()
        .map(|s| Form::of(&s))
        .map_err(|e| user_messages::for_error(&e))
}

/// Blocking (op thread). `Ok` carries the stored values (what the screen shows afterwards).
pub fn save(core: &Arc<Core>, form: &Form) -> Result<Form, String> {
    let svc = core.service().ok_or("The engine is not ready yet.")?;
    let cur = svc.settings().map_err(|e| user_messages::for_error(&e))?;
    let next = apply(&cur, form);
    if next == cur {
        return Ok(Form::of(&cur));
    }
    svc.set_settings(next.clone())
        .map_err(|e| user_messages::for_error(&e))?;
    if clip_mode_changed(&cur, &next)
        && let Some(e) = clipboard_runtime::engine(core)
    {
        e.refresh_mode();
    }
    if needs_retention_sweep(&cur, &next) {
        inbox_runtime::retention_soon(core);
    }
    Ok(Form::of(&next))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untouched_form_changes_nothing_even_for_odd_byte_values() {
        let cur = Settings {
            retention_bytes: 1_500_000, // not a MiB multiple
            ..Settings::default()
        };
        let f = Form::of(&cur);
        assert_eq!(apply(&cur, &f), cur);
    }

    #[test]
    fn edits_map_to_settings_and_the_denylist_is_cleaned() {
        let cur = Settings::default();
        let mut f = Form::of(&cur);
        f.manual = true;
        f.retention_mib = 10;
        f.max_clip_mib = 5;
        f.retention_days = 7;
        f.auto_accept = true;
        f.denylist = " keepassxc \n\nkeepassxc\nfirefox".into();
        let s = apply(&cur, &f);
        assert_eq!(s.clip_mode, ClipMode::Manual);
        assert_eq!((s.retention_bytes, s.max_clip_bytes), (10 * MIB, 5 * MIB));
        assert_eq!(s.retention_days, 7);
        assert!(s.auto_accept_files);
        assert_eq!(s.denylist, vec!["keepassxc", "firefox"]);
        assert_eq!(Form::of(&s).denylist, "keepassxc\nfirefox");
    }

    #[test]
    fn clip_mode_and_auto_accept_switch_both_ways() {
        let cur = Settings {
            clip_mode: ClipMode::Manual,
            auto_accept_files: true,
            ..Settings::default()
        };
        let mut f = Form::of(&cur);
        assert!(f.manual && f.auto_accept);
        f.manual = false;
        f.auto_accept = false;
        let s = apply(&cur, &f);
        assert_eq!(s.clip_mode, ClipMode::Auto);
        assert!(!s.auto_accept_files);
    }

    #[test]
    fn huge_size_saturates_instead_of_wrapping() {
        let cur = Settings::default();
        let mut f = Form::of(&cur);
        f.retention_mib = u64::MAX;
        assert_eq!(apply(&cur, &f).retention_bytes, u64::MAX);
    }

    #[test]
    fn a_size_the_box_cannot_show_is_kept_exactly_until_the_user_edits_it() {
        let huge = u64::from(u32::MAX) * 4; // MiB, above i32::MAX
        assert_eq!(resolve_mib(i64::from(i32::MAX), huge), huge);
        assert_eq!(resolve_mib(500, 500), 500);
        assert_eq!(resolve_mib(7, 500), 7);
        assert_eq!(resolve_mib(-3, 500), 0);
    }

    #[test]
    fn side_effects_follow_the_fields_that_changed() {
        let cur = Settings::default();
        let mut next = cur.clone();
        assert!(!needs_retention_sweep(&cur, &next) && !clip_mode_changed(&cur, &next));
        next.denylist.push("x".into());
        next.max_clip_bytes += 1;
        next.auto_accept_files = true;
        assert!(!needs_retention_sweep(&cur, &next) && !clip_mode_changed(&cur, &next));
        next.retention_days += 1;
        assert!(needs_retention_sweep(&cur, &next));
        let mut next = cur.clone();
        next.retention_bytes += 1;
        assert!(needs_retention_sweep(&cur, &next));
        next.clip_mode = ClipMode::Manual;
        assert!(clip_mode_changed(&cur, &next));
    }
}
