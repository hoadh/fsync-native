//! Validation of peer-supplied names and paths (file names, relative paths, device names).
//! Receivers must run these before touching the filesystem or showing a name in the UI.

use crate::error::ProtoError;

pub const MAX_COMPONENT_BYTES: usize = 255;
pub const MAX_REL_PATH_BYTES: usize = 1024;
pub const MAX_DEVICE_NAME_CHARS: usize = 64;

/// Windows device names, reserved with or without an extension (case-insensitive), including
/// the superscript forms COM¹²³ / LPT¹²³ and the console handles CONIN$ / CONOUT$.
fn is_reserved_windows(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    let mut chars = stem.chars();
    let prefix: String = chars.by_ref().take(3).collect();
    matches!(prefix.as_str(), "COM" | "LPT")
        && matches!(
            chars.next(),
            Some('1'..='9' | '\u{B9}' | '\u{B2}' | '\u{B3}')
        )
        && chars.next().is_none()
}

/// One path component: no separators, Windows-forbidden characters (`: < > " | ? *`), control
/// or bidi/invisible characters, trailing dot/space, reserved names.
pub fn validate_component(c: &str) -> Result<(), ProtoError> {
    let bad = c.is_empty()
        || c.len() > MAX_COMPONENT_BYTES
        || c == "."
        || c == ".."
        || c.ends_with('.')
        || c.ends_with(' ')
        || c.chars().any(|ch| {
            matches!(ch, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
                || ch.is_control()
                || is_bidi_or_invisible(ch)
        })
        || is_reserved_windows(c);
    if bad {
        Err(ProtoError::InvalidPath)
    } else {
        Ok(())
    }
}

/// `FileMeta::name`: exactly one safe component.
pub fn validate_file_name(name: &str) -> Result<(), ProtoError> {
    validate_component(name)
}

/// Peer-supplied relative paths must stay inside the destination directory.
pub fn validate_rel_path(p: &str) -> Result<(), ProtoError> {
    if p.is_empty() || p.len() > MAX_REL_PATH_BYTES || p.starts_with('/') {
        return Err(ProtoError::InvalidPath);
    }
    p.split('/').try_for_each(validate_component)
}

/// Bidi controls and invisible characters that can disguise a displayed name.
fn is_bidi_or_invisible(c: char) -> bool {
    matches!(c, '\u{00AD}' | '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}'
        | '\u{2028}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}'
        | '\u{E0000}'..='\u{E007F}')
}

/// Device display names: 1..=64 chars, no control or bidi/invisible characters.
pub fn validate_device_name(name: &str) -> Result<(), ProtoError> {
    let n = name.chars().count();
    if n == 0 || n > MAX_DEVICE_NAME_CHARS {
        return Err(ProtoError::InvalidDeviceList("device name length"));
    }
    if name
        .chars()
        .any(|c| c.is_control() || is_bidi_or_invisible(c))
    {
        return Err(ProtoError::InvalidDeviceList("device name characters"));
    }
    Ok(())
}
