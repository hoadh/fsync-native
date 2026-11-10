//! Names that may become directory or file names in the inbox. Everything that came from the
//! network is checked here.

use super::inbox::{COMPLETE, EXPLICIT};

/// Item ids are UUIDs (hex and dashes); nothing else becomes a directory name.
pub fn valid_item_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f' | b'-'))
}

/// Our marker files (and their temp names) never come from the network.
fn is_reserved(name: &str) -> bool {
    name.starts_with(COMPLETE) || name.starts_with(EXPLICIT)
}

/// One path component of a received name: no separators, NUL, line breaks, `.`/`..`, the
/// reserved marker name, and at most 255 bytes.
pub fn valid_component(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name != "."
        && name != ".."
        && !is_reserved(name)
        && !name.bytes().any(|b| matches!(b, b'/' | 0 | b'\n' | b'\r'))
}

pub fn valid_rel_path(rel: &str) -> bool {
    rel.split('/').all(valid_component)
}
