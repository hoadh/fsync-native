//! `text/uri-list` and `x-special/gnome-copied-files`: parsing (local files only) and building.

use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

fn percent_decode(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let pair = b.get(i + 1..i + 3)?;
            // Strict: `from_str_radix` alone would accept a sign ("%+1").
            if !pair.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            out.push(u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Some(out)
}

/// Local absolute paths from a `text/uri-list` (RFC 2483: CRLF lines, `#` comments) or the
/// body of `x-special/gnome-copied-files` (first line is `copy` / `cut`). Remote hosts, other
/// schemes, relative paths and NUL bytes are dropped.
pub fn parse_uri_list(data: &[u8], gnome_style: bool) -> Vec<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let Ok(text) = std::str::from_utf8(data) else {
        return Vec::new();
    };
    // The first line of gnome-copied-files is the action; a file URI there is kept.
    let skip = gnome_style && !text.trim_start().starts_with("file://");
    let lines = text.lines().skip(usize::from(skip));
    let mut out = Vec::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(rest) = line.strip_prefix("file://") else {
            continue;
        };
        // "file:///x" (empty host), "file://localhost/x"; any other host is remote.
        let path_part = match rest.find('/') {
            Some(0) => rest,
            Some(i) if rest[..i].eq_ignore_ascii_case("localhost") => &rest[i..],
            _ => continue,
        };
        let Some(bytes) = percent_decode(path_part) else {
            continue;
        };
        if bytes.contains(&0) || bytes.first() != Some(&b'/') {
            continue;
        }
        out.push(PathBuf::from(std::ffi::OsString::from_vec(bytes)));
    }
    out
}

/// `file://` URI of an absolute path: every byte except RFC 3986 unreserved and `/` is
/// percent-encoded (names are arbitrary bytes on Linux).
pub fn file_uri(path: &Path) -> String {
    let mut out = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `text/uri-list` body (RFC 2483: one URI per CRLF-terminated line).
pub fn build_uri_list(paths: &[PathBuf]) -> Vec<u8> {
    let mut out = String::new();
    for p in paths {
        out.push_str(&file_uri(p));
        out.push_str("\r\n");
    }
    out.into_bytes()
}

/// `x-special/gnome-copied-files` body: the action line, then URIs, `\n` separated and without
/// a trailing newline (what Nautilus writes and expects).
pub fn build_gnome_copied_files(paths: &[PathBuf]) -> Vec<u8> {
    let mut out = String::from("copy");
    for p in paths {
        out.push('\n');
        out.push_str(&file_uri(p));
    }
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_lists_parse_back_even_for_awkward_names() {
        let paths: Vec<PathBuf> = vec![
            "/tmp/plain.txt".into(),
            "/tmp/with space/ü #%?.txt".into(),
            std::ffi::OsStr::from_bytes(b"/tmp/\xff\xfe-raw").into(),
        ];
        let uri = build_uri_list(&paths);
        assert!(uri.ends_with(b"\r\n"));
        assert_eq!(parse_uri_list(&uri, false), paths);
        let gnome = build_gnome_copied_files(&paths);
        assert!(gnome.starts_with(b"copy\nfile:///tmp/plain.txt\nfile:///tmp/with%20space"));
        assert!(!gnome.ends_with(b"\n"));
        assert_eq!(parse_uri_list(&gnome, true), paths);
    }

    #[test]
    fn uri_escapes_the_characters_that_break_uris() {
        assert_eq!(file_uri(Path::new("/a b/c#d%e")), "file:///a%20b/c%23d%25e");
    }
}
