//! Generates the C++ side of the `#[cxx_qt::bridge]` modules, compiles the QML module
//! `app.fsync` (qmlcachegen on Qt 6) and links Qt. Also: build-info env and the test-feature guard cfg.
use cxx_qt_build::{CxxQtBuilder, QmlModule};
use std::process::Command;

const QML_FILES: [&str; 19] = [
    "qml/Main.qml",
    "qml/SignInView.qml",
    "qml/PairingView.qml",
    "qml/SasDialog.qml",
    "qml/DevicesView.qml",
    "qml/TrustResetDialog.qml",
    "qml/StatusBanner.qml",
    "qml/SettingsView.qml",
    "qml/ResetHistoryDialog.qml",
    "qml/ClipboardStatus.qml",
    "qml/QuickClipWindow.qml",
    "qml/FirstRunNotice.qml",
    "qml/ErrorView.qml",
    "qml/HistoryView.qml",
    "qml/OffersView.qml",
    "qml/SettingsClipboardForm.qml",
    "qml/ShortcutHelp.qml",
    "qml/DevicePicker.qml",
    "qml/SendDropArea.qml",
];

fn git_output(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (out.status.success() && !text.is_empty()).then_some(text)
}

fn main() {
    // e2e-control (SAS bypass) may not be built in release mode unless a test pipeline opts in.
    println!("cargo::rustc-check-cfg=cfg(fsync_allow_test_features)");
    println!("cargo::rerun-if-env-changed=FSYNC_ALLOW_TEST_FEATURES");
    if std::env::var("FSYNC_ALLOW_TEST_FEATURES").is_ok_and(|v| v == "1") {
        println!("cargo::rustc-cfg=fsync_allow_test_features");
    }

    // build-release.sh passes FSYNC_GIT_SHA (computed on the host, with the -dirty mark); otherwise
    // ask git here. A tree with uncommitted changes is marked "-dirty" so a package never looks
    // like a clean commit (the mark is only refreshed when HEAD, the index or the env changes).
    println!("cargo::rerun-if-env-changed=FSYNC_GIT_SHA");
    let sha = std::env::var("FSYNC_GIT_SHA").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| {
        let head = git_output(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".into());
        let dirty = git_output(&["status", "--porcelain"]).is_some();
        if dirty { format!("{head}-dirty") } else { head }
    });
    println!("cargo::rustc-env=FSYNC_GIT_SHA={sha}");
    for what in ["HEAD", "index"] {
        if let Some(path) = git_output(&["rev-parse", "--git-path", what]) {
            println!("cargo::rerun-if-changed={path}");
        }
    }

    // cpp_file() tracks the .cpp only; the header must trigger a rebuild too.
    println!("cargo::rerun-if-changed=cpp/smoke_log.h");
    println!("cargo::rerun-if-changed=cpp/app_identity.h");
    println!("cargo::rerun-if-changed=cpp/clip_access.h");
    let mut module = QmlModule::new("app.fsync");
    for f in QML_FILES {
        module = module.qml_file(f);
    }
    CxxQtBuilder::new_qml_module(module)
        .qt_module("Quick")
        .files([
            "src/bridge/engine_object.rs",
            "src/bridge/pairing_object.rs",
            "src/bridge/devices_model.rs",
            "src/bridge/smoke_log.rs",
            "src/bridge/app_identity.rs",
            "src/bridge/clip_access.rs",
            "src/bridge/clipboard_object.rs",
            "src/bridge/quick_clip.rs",
            "src/bridge/history_model.rs",
            "src/bridge/transfers_object.rs",
            "src/bridge/settings_object.rs",
            "src/bridge/send_object.rs",
        ])
        .cpp_file("cpp/smoke_log.cpp")
        .cpp_file("cpp/app_identity.cpp")
        .cpp_file("cpp/clip_access.cpp")
        .build();
}
