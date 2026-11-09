//! Our own writes must never be published again. Two guards: the marker MIME on every entry
//! (`is_own`, no read needed) and a content-hash of the last entry we wrote (`WriteGuard`, for
//! owners that drop the marker). Core's `Published.echo` is the third.

use super::{MARKER_MIME, OwnedEntry};
use std::sync::Mutex;

pub fn is_own(mimes: &[String]) -> bool {
    mimes.iter().any(|m| m.eq_ignore_ascii_case(MARKER_MIME))
}

/// Hash over `(mime, bytes)` pairs in order; used on the entry we write and on what we read.
pub fn rep_set_hash<'a>(reps: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    for (mime, data) in reps {
        h.update(&(mime.len() as u64).to_le_bytes());
        h.update(mime.as_bytes());
        h.update(&(data.len() as u64).to_le_bytes());
        h.update(data);
    }
    *h.finalize().as_bytes()
}

/// Content reps of an entry (not the marker or hint), hashed like a read clip.
pub fn entry_hash(entry: &OwnedEntry) -> [u8; 32] {
    rep_set_hash(
        entry
            .reps
            .iter()
            .filter(|r| !r.is_internal())
            .map(|r| (r.mime.as_str(), r.data.as_slice())),
    )
}

#[derive(Clone, Copy)]
struct Written {
    full: [u8; 32],
    /// Hash of the text rep alone: a text-only clipboard manager re-owns the selection with
    /// just the text of a rich entry we wrote.
    text: Option<[u8; 32]>,
}

#[derive(Default)]
pub struct WriteGuard {
    last: Mutex<Option<Written>>,
}

impl WriteGuard {
    pub fn remember(&self, entry: &OwnedEntry) {
        let text = entry
            .reps
            .iter()
            .find(|r| r.mime == "text/plain")
            .map(|r| rep_set_hash([("text/plain", r.data.as_slice())]));
        *self.last.lock().unwrap_or_else(|p| p.into_inner()) = Some(Written {
            full: entry_hash(entry),
            text,
        });
    }

    /// The marker proved the echo already: the content guard must not eat a later real copy.
    pub fn clear(&self) {
        *self.last.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }

    /// True for the first read after our write if it equals what we wrote (or its text alone).
    /// The guard is spent by that first read whatever it was: a later copy of the same content
    /// is a real copy and must be published.
    pub fn is_echo(&self, reps: &[(&str, &[u8])]) -> bool {
        let Some(w) = self.last.lock().unwrap_or_else(|p| p.into_inner()).take() else {
            return false;
        };
        if rep_set_hash(reps.iter().copied()) == w.full {
            return true;
        }
        matches!(reps, [("text/plain", data)] if Some(rep_set_hash([("text/plain", *data)])) == w.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::EntryRep;

    fn entry(text: &str) -> OwnedEntry {
        OwnedEntry {
            reps: vec![
                EntryRep {
                    mime: "text/plain".into(),
                    offered: vec!["text/plain".into(), "STRING".into()],
                    data: text.into(),
                },
                EntryRep {
                    mime: MARKER_MIME.into(),
                    offered: vec![MARKER_MIME.into()],
                    data: b"1".to_vec(),
                },
            ],
        }
    }

    #[test]
    fn marker_is_detected_without_reading() {
        assert!(is_own(&["text/plain".into(), MARKER_MIME.into()]));
        assert!(!is_own(&["text/plain".into()]));
    }

    #[test]
    fn guard_is_spent_by_the_first_read() {
        let g = WriteGuard::default();
        g.remember(&entry("abc"));
        let same = [("text/plain", &b"abc"[..])];
        assert!(g.is_echo(&same));
        assert!(!g.is_echo(&same), "spent after one match");
        // A different first read spends the guard too: a later identical copy is real.
        g.remember(&entry("abc"));
        assert!(!g.is_echo(&[("text/plain", &b"zzz"[..])]));
        assert!(!g.is_echo(&same));
    }

    #[test]
    fn hash_depends_on_mime_and_bytes() {
        let a = rep_set_hash([("text/plain", &b"x"[..])]);
        assert_ne!(a, rep_set_hash([("text/html", &b"x"[..])]));
        assert_ne!(a, rep_set_hash([("text/plain", &b"y"[..])]));
    }
}

#[cfg(test)]
mod text_only_tests {
    use super::*;
    use crate::clipboard::EntryRep;

    #[test]
    fn a_text_only_re_own_of_a_rich_entry_is_an_echo() {
        let rep = |m: &str, d: &[u8]| EntryRep {
            mime: m.into(),
            offered: vec![m.into()],
            data: d.to_vec(),
        };
        let g = WriteGuard::default();
        g.remember(&OwnedEntry {
            reps: vec![rep("text/html", b"<b>x</b>"), rep("text/plain", b"x")],
        });
        assert!(g.is_echo(&[("text/plain", &b"x"[..])]));
    }
}
