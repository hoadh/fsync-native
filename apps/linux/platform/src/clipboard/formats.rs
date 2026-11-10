//! Clipboard format map (ADR 013 decision 2 counterpart). Out, in rep order (core derives the
//! clip kind from the first rep): `text/html` (only together with text), `text/plain`,
//! `image/png`; `text/uri-list` becomes files. In: the reverse, text under all its aliases.

pub use super::uri_list::{build_gnome_copied_files, build_uri_list, parse_uri_list};
use super::{EntryRep, KDE_HINT_MIME, MARKER_MIME, OwnedEntry};
use fsync_core::service::ClipRep;

pub const MAX_IMAGE_SIDE: u32 = 16384;
pub const MAX_IMAGE_PIXELS: u64 = 64_000_000;

/// Preference order for reading text (UTF-8 names first; STRING/TEXT are not UTF-8 on X11).
pub const TEXT_READ_MIMES: &[&str] = &["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];
/// Names a text rep is offered under when we write it.
pub const TEXT_ALIASES: &[&str] = &[
    "text/plain;charset=utf-8",
    "UTF8_STRING",
    "STRING",
    "TEXT",
    "text/plain",
];
pub const GNOME_FILES_MIME: &str = "x-special/gnome-copied-files";
/// A file manager sets one of these next to `text/uri-list`; a bare uri-list can come from any
/// client (a web page through the browser), so it alone never publishes files.
pub const FILE_MANAGER_MIMES: &[&str] = &[GNOME_FILES_MIME, "application/x-kde-cutselection"];

/// Which of the offered MIME types we would read, decided on names only (before any bytes).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReadPlan {
    pub text: Option<String>,
    pub html: Option<String>,
    pub png: Option<String>,
    pub uri_list: Option<String>,
    pub gnome_files: Option<String>,
    pub file_manager: bool,
}

impl ReadPlan {
    pub fn from_mimes(mimes: &[String]) -> Self {
        // The offered spelling is what gets read; names compare case-insensitively.
        let find = |m: &str| mimes.iter().find(|x| x.eq_ignore_ascii_case(m)).cloned();
        let text = TEXT_READ_MIMES.iter().find_map(|m| find(m));
        Self {
            html: text.as_ref().and_then(|_| find("text/html")),
            png: find("image/png"),
            uri_list: find("text/uri-list"),
            gnome_files: find("x-special/gnome-copied-files"),
            file_manager: FILE_MANAGER_MIMES.iter().any(|m| find(m).is_some()),
            text,
        }
    }

    /// Files are considered only with a file-manager type present.
    pub fn wants_files(&self) -> bool {
        self.file_manager && (self.uri_list.is_some() || self.gnome_files.is_some())
    }

    pub fn wants_content(&self) -> bool {
        self.text.is_some() || self.png.is_some()
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FormatSkip {
    #[error("nothing to sync")]
    NoContent,
    #[error("image is not a valid PNG or is too large")]
    BadImage,
}

/// Bytes read for each planned MIME.
#[derive(Default)]
pub struct RawReps {
    pub text: Option<Vec<u8>>,
    pub html: Option<Vec<u8>>,
    pub png: Option<Vec<u8>>,
}

/// `(width, height)` from the PNG signature and IHDR; `None` when it is not a PNG.
pub fn png_dims(data: &[u8]) -> Option<(u32, u32)> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if data.len() < 24 || data[..8] != SIG || &data[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(data[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(data[20..24].try_into().ok()?);
    Some((w, h))
}

pub fn png_within_bounds(data: &[u8]) -> bool {
    match png_dims(data) {
        Some((w, h)) => {
            w > 0
                && h > 0
                && w <= MAX_IMAGE_SIDE
                && h <= MAX_IMAGE_SIDE
                && u64::from(w) * u64::from(h) <= MAX_IMAGE_PIXELS
        }
        None => false,
    }
}

fn utf8_nonempty(data: Vec<u8>) -> Option<Vec<u8>> {
    let s = std::str::from_utf8(&data).ok()?;
    (!s.is_empty()).then_some(data)
}

/// Outgoing reps in wire order. Invalid text or HTML is dropped quietly (another encoding);
/// an invalid or oversized PNG fails the whole clip (`BadImage`) so the user sees why.
pub fn build_reps(raw: RawReps) -> Result<Vec<ClipRep>, FormatSkip> {
    let mut reps = Vec::new();
    let text = raw.text.and_then(utf8_nonempty);
    if let (Some(_), Some(html)) = (&text, raw.html.and_then(utf8_nonempty)) {
        reps.push(ClipRep {
            mime: "text/html".into(),
            data: html,
        });
    }
    if let Some(t) = text {
        reps.push(ClipRep {
            mime: "text/plain".into(),
            data: t,
        });
    }
    if let Some(png) = raw.png {
        if !png_within_bounds(&png) {
            return Err(FormatSkip::BadImage);
        }
        reps.push(ClipRep {
            mime: "image/png".into(),
            data: png,
        });
    }
    if reps.is_empty() {
        return Err(FormatSkip::NoContent);
    }
    Ok(reps)
}

/// What we write for a received clip: content reps with aliases, then the marker and the
/// privacy hint (received content stays out of clipboard managers). Unknown MIME types are
/// not written; an image outside the bounds is skipped. Empty result = nothing writable.
pub fn to_entry(reps: &[ClipRep]) -> OwnedEntry {
    let mut out = Vec::new();
    for r in reps {
        let offered: Vec<String> = match r.mime.as_str() {
            "text/plain" if std::str::from_utf8(&r.data).is_ok() => {
                TEXT_ALIASES.iter().map(|s| (*s).to_owned()).collect()
            }
            "text/html" => vec!["text/html".into()],
            "image/png" if png_within_bounds(&r.data) => vec!["image/png".into()],
            _ => continue,
        };
        out.push(EntryRep {
            mime: r.mime.clone(),
            offered,
            data: r.data.clone(),
        });
    }
    if out.is_empty() {
        return OwnedEntry::default();
    }
    out.push(EntryRep {
        mime: MARKER_MIME.into(),
        offered: vec![MARKER_MIME.into()],
        data: b"1".to_vec(),
    });
    out.push(EntryRep {
        mime: KDE_HINT_MIME.into(),
        offered: vec![KDE_HINT_MIME.into()],
        data: b"secret".to_vec(),
    });
    OwnedEntry { reps: out }
}

/// What we write for received files that sit in the inbox: the paths as `text/uri-list` and as
/// `x-special/gnome-copied-files` (file managers need one of them; our own file-manager marker
/// also keeps a second fsync instance treating the entry as a file-manager copy), the marker and
/// the privacy hint. Empty `paths` give an empty entry.
pub fn files_entry(paths: &[std::path::PathBuf]) -> OwnedEntry {
    if paths.is_empty() {
        return OwnedEntry::default();
    }
    let rep = |mime: &str, data: Vec<u8>| EntryRep {
        mime: mime.to_owned(),
        offered: vec![mime.to_owned()],
        data,
    };
    OwnedEntry {
        reps: vec![
            rep("text/uri-list", build_uri_list(paths)),
            rep(GNOME_FILES_MIME, build_gnome_copied_files(paths)),
            rep(MARKER_MIME, b"1".to_vec()),
            rep(KDE_HINT_MIME, b"secret".to_vec()),
        ],
    }
}

#[cfg(test)]
#[path = "formats_tests.rs"]
mod tests;
