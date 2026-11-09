//! Rust side of `cpp/app_identity.cpp`: desktop file name and the xdg-activation token.

#[cxx::bridge(namespace = "fsync_identity")]
mod ffi {
    unsafe extern "C++" {
        include!("fsync-linux/cpp/app_identity.h");
        fn set_desktop_file_name(name: &str);
        fn set_activation_token(token: &str) -> bool;
        fn clear_activation_token();
    }
}

/// Without this the Wayland app_id is the binary name (`fsync-linux`) and compositors cannot
/// match the window to `app.fsync.Fsync.desktop` (icon, StartupWMClass). Call after
/// `QGuiApplication` exists and before the first window.
pub fn set_desktop_file_name() {
    ffi::set_desktop_file_name(fsync_linux_platform::APP_ID);
}

/// Longest token a compositor hands out is far below this; anything else is not one.
const MAX_TOKEN_BYTES: usize = 512;

fn is_plausible_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= MAX_TOKEN_BYTES
        && token.bytes().all(|b| b.is_ascii_graphic())
}

/// Puts the launcher's activation token in `XDG_ACTIVATION_TOKEN` for the next `requestActivate`.
/// Qt >= 6.3 (Wayland) reads and unsets it there. On a runtime older than 6.3 the C++ side
/// leaves the environment alone (nothing would read it, and setenv races getenv on other
/// threads); on X11 the token is simply unused. Returns whether the environment was changed (then `clear_activation_token`
/// must follow right after the activation, so a stale token never leaks into later windows or
/// child processes). Wayland only honours a token that came from the user's launch action.
pub fn set_activation_token(token: &str) -> bool {
    if !is_plausible_token(token) {
        return false;
    }
    ffi::set_activation_token(token)
}

pub fn clear_activation_token() {
    ffi::clear_activation_token();
}

#[cfg(test)]
mod tests {
    use super::is_plausible_token;

    #[test]
    fn only_printable_ascii_tokens_of_sane_length_pass() {
        assert!(is_plausible_token("a1b2-C3_d4"));
        assert!(!is_plausible_token(""));
        assert!(!is_plausible_token("has space"));
        assert!(!is_plausible_token("nul\0byte"));
        assert!(!is_plausible_token(&"x".repeat(513)));
    }
}
