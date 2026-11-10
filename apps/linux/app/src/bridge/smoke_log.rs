//! Rust side of `cpp/smoke_log.cpp` (QML warning counter used by `--smoke`).

#[cxx::bridge(namespace = "fsync_smoke")]
mod ffi {
    unsafe extern "C++" {
        include!("fsync-linux/cpp/smoke_log.h");
        fn install_handler();
        fn qml_warning_count() -> i32;
    }
}

/// Installs the counting message handler (chains to Qt's default output).
pub fn install() {
    ffi::install_handler();
}

/// QML warnings or errors seen since `install`.
pub fn qml_warnings() -> i32 {
    ffi::qml_warning_count()
}
