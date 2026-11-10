//! Linux OS integration for the fsync desktop app. No Qt here: everything in this crate is
//! unit-testable without a display. Modules arrive per phase (paths, secrets, ipc: 02;
//! clipboard, sync: 03; files: 04a).

// Mirrors the app crate's guard: the SAS-bypassing control commands never ship in release.
#[cfg(all(
    feature = "e2e-control",
    not(debug_assertions),
    not(fsync_allow_test_features)
))]
compile_error!(
    "e2e-control is test-only: it must not be built in release mode \
     (set FSYNC_ALLOW_TEST_FEATURES=1 for a test-only release build)"
);

pub mod autostart;
pub mod clipboard;
pub mod config_file;
pub mod files;
pub mod instance_lock;
pub mod ipc;
pub mod notify;
pub mod paths;
pub mod secrets;
pub mod sync;

/// Cargo features this crate was built with that a release must not carry (`--build-info`).
pub const BUILT_FEATURES: &[&str] = &[
    #[cfg(feature = "e2e-control")]
    "e2e-control",
];

/// Reverse-DNS application id (desktop file, D-Bus names, Secret Service attributes).
pub const APP_ID: &str = "app.fsync.Fsync";

#[cfg(test)]
mod tests {
    #[test]
    fn app_id_is_reverse_dns() {
        assert_eq!(super::APP_ID.split('.').count(), 3);
    }
}
