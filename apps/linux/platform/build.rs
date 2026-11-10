//! Same release guard cfg as the app crate: `e2e-control` in this crate is the SAS-bypassing IPC
//! command set, so it must be refused in release builds unless a test pipeline opts in.
fn main() {
    println!("cargo::rustc-check-cfg=cfg(fsync_allow_test_features)");
    println!("cargo::rerun-if-env-changed=FSYNC_ALLOW_TEST_FEATURES");
    if std::env::var("FSYNC_ALLOW_TEST_FEATURES").is_ok_and(|v| v == "1") {
        println!("cargo::rustc-cfg=fsync_allow_test_features");
    }
}
