//! Files on the Linux desktop: the eager inbox for received file clips, its retention, the
//! download directory for pushed files and the wait for a core transfer to finish. No Qt.
//!
//! Linux has no virtual-file paste, so a received file clip is always downloaded completely into
//! `<data_dir>/inbox/<item_id>/` and then offered as `text/uri-list` plus
//! `x-special/gnome-copied-files` (see `clipboard::formats::files_entry`).

pub mod download_dir;
pub mod inbox;
mod inbox_claims;
mod names;
pub mod retention;
pub mod transfer_wait;

/// Largest file clip fetched without asking (ADR 009 / 013 d6): bigger ones wait in History.
pub const EAGER_LIMIT: u64 = 20 << 20;
/// Pushed files up to this size are accepted without a prompt when auto-accept is on (d14).
pub const AUTO_ACCEPT_LIMIT: u64 = 1 << 30;

/// Whether a pushed offer is accepted without asking: the setting is on and the total is at
/// most 1 GiB (inclusive).
pub fn should_auto_accept(setting_on: bool, total_bytes: u64) -> bool {
    setting_on && total_bytes <= AUTO_ACCEPT_LIMIT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_accept_needs_the_setting_and_stops_above_one_gib() {
        assert!(should_auto_accept(true, 0));
        assert!(should_auto_accept(true, AUTO_ACCEPT_LIMIT));
        assert!(!should_auto_accept(true, AUTO_ACCEPT_LIMIT + 1));
        assert!(!should_auto_accept(false, 1));
    }
}
