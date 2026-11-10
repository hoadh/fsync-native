//! Adversarial pass, part 1 (QA, 2026-11-06): uri-list parsing, MIME plan, PNG bounds and the
//! path denylist against real files and symlinks. No display needed. The former known gaps (hint
//! and charset case) are fixed and run as normal tests.
mod common;

use common::clip_fakes::{denylist_for, home_with_secrets};
use fsync_core::service::ClipRep;
use fsync_linux_platform::clipboard::denylist_builtin::effective;
use fsync_linux_platform::clipboard::filters::{Skip, classify_mimes, classify_source};
use fsync_linux_platform::clipboard::formats::{
    FormatSkip, RawReps, ReadPlan, build_reps, parse_uri_list, png_within_bounds, to_entry,
};
use fsync_linux_platform::clipboard::path_denylist::PathDenied;
use fsync_linux_platform::clipboard::{KDE_HINT_MIME, MARKER_MIME, SourceApp};

// ---------- pure rules (no display) ----------

fn png_header(w: u32, h: u32, tag: &[u8; 4]) -> Vec<u8> {
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    v.extend_from_slice(&13u32.to_be_bytes());
    v.extend_from_slice(tag);
    v.extend_from_slice(&w.to_be_bytes());
    v.extend_from_slice(&h.to_be_bytes());
    v.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // rest of IHDR + a CRC stub
    v
}

#[test]
fn uri_list_parser_rules_hold_for_hostile_lines() {
    let paths = |s: &str, g: bool| -> Vec<String> {
        parse_uri_list(s.as_bytes(), g)
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    };
    // CRLF, comments, blank lines, localhost host (case-insensitive) and %2F all work.
    assert_eq!(
        paths(
            "# c\r\nfile:///a/b\r\n\r\nfile://LOCALHOST/c%2Fd\r\n",
            false
        ),
        ["/a/b", "/c/d"]
    );
    // Remote hosts, other schemes, relative and scheme-less lines are dropped.
    assert!(paths("file://remote.host/etc/passwd\n", false).is_empty());
    assert!(paths("file://relative/x\n", false).is_empty());
    assert!(paths("file:relative\n", false).is_empty());
    assert!(paths("/abs/no-scheme\n", false).is_empty());
    assert!(paths("https://example.com/x\n", false).is_empty());
    // NUL bytes (any encoding), broken percent escapes and non-UTF-8 lists are dropped.
    assert!(paths("file:///a%00b\n", false).is_empty());
    assert!(paths("file:///a%zz\n", false).is_empty());
    assert!(paths("file:///a%\n", false).is_empty());
    assert!(parse_uri_list(&[0xff, b'\n', b'f'], false).is_empty());
    // Dot segments survive the parser; they are refused by the path check (below).
    assert_eq!(
        paths("file:///home/u/Documents/../.ssh/id\n", false).len(),
        1
    );
    // gnome-copied-files: the first line is the action.
    assert_eq!(paths("copy\nfile:///a\nfile:///b\n", true), ["/a", "/b"]);
}

#[test]
fn png_header_bounds_are_enforced_before_any_decode() {
    // Exactly at the limits: 16384 x 3906 = 63 995 904 px (<= 64 MP) is accepted.
    assert!(png_within_bounds(&png_header(16384, 3906, b"IHDR")));
    assert!(png_within_bounds(&png_header(1, 1, b"IHDR")));
    // One row over the pixel budget, one px over the side limit.
    assert!(!png_within_bounds(&png_header(16384, 3907, b"IHDR")));
    assert!(!png_within_bounds(&png_header(16385, 1, b"IHDR")));
    // Huge and zero dimensions: no overflow, rejected.
    assert!(!png_within_bounds(&png_header(u32::MAX, u32::MAX, b"IHDR")));
    assert!(!png_within_bounds(&png_header(0, 100, b"IHDR")));
    // Truncated header (IHDR cut before the height), bad chunk name, bad signature.
    let full = png_header(10, 10, b"IHDR");
    assert!(!png_within_bounds(&full[..23]));
    assert!(!png_within_bounds(&png_header(10, 10, b"IDAT")));
    let mut bad_sig = full.clone();
    bad_sig[1] = b'X';
    assert!(!png_within_bounds(&bad_sig));
    // Oversized PNG fails the whole clip (BadImage), not just the image rep.
    let raw = RawReps {
        text: Some(b"hi".to_vec()),
        html: None,
        png: Some(png_header(20000, 20000, b"IHDR")),
    };
    assert!(matches!(build_reps(raw), Err(FormatSkip::BadImage)));
    // A bad image never reaches the clipboard on the inbound side either.
    let inbound = ClipRep {
        mime: "image/png".into(),
        data: png_header(20000, 20000, b"IHDR"),
    };
    assert!(to_entry(&[inbound]).reps.is_empty());
}

#[test]
fn mime_plan_needs_a_file_manager_type_and_exact_utf8_names() {
    let m = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    // A bare uri-list (any client can set it) never publishes files.
    assert!(!ReadPlan::from_mimes(&m(&["text/uri-list"])).wants_files());
    assert!(
        ReadPlan::from_mimes(&m(&["text/uri-list", "application/x-kde-cutselection"]))
            .wants_files()
    );
    // Hints and the own marker win over any content.
    assert_eq!(
        classify_mimes(&m(&["text/plain", KDE_HINT_MIME])),
        Err(Skip::Hinted)
    );
    assert_eq!(
        classify_mimes(&m(&["text/plain", MARKER_MIME])),
        Err(Skip::OwnEcho)
    );
    // Source denylist: case-insensitive over name, class and app id.
    let list = effective(&[]);
    for name in [
        "KeePassXC",
        "1Password",
        "Bitwarden",
        "org.keepassxc.KeePassXC",
    ] {
        let app = SourceApp {
            classes: vec![name.to_owned()],
            ..Default::default()
        };
        assert_eq!(
            classify_source(Some(&app), &list),
            Err(Skip::DeniedApp),
            "{name}"
        );
    }
    // Unknown source is allowed (documented limit).
    assert_eq!(classify_source(None, &list), Ok(()));
}

#[test]
fn gap_hint_mime_in_another_case_must_still_be_hinted() {
    assert_eq!(
        classify_mimes(&[
            "text/plain".to_owned(),
            "X-KDE-PASSWORDMANAGERHINT".to_owned()
        ]),
        Err(Skip::Hinted)
    );
}

#[test]
fn gap_text_offered_with_upper_case_charset_is_read() {
    let plan = ReadPlan::from_mimes(&["text/plain;charset=UTF-8".to_owned()]);
    assert!(plan.wants_content(), "plan = {plan:?}");
}

#[test]
fn dot_dot_and_symlinks_cannot_reach_protected_paths() {
    let h = home_with_secrets();
    let list = denylist_for(&h);
    // Lexically inside Documents, physically in ~/.ssh.
    let traversal = h.home.join("Documents/../.ssh/id_ed25519");
    assert_eq!(list.check(&traversal), Err(PathDenied::Protected));
    // Symlink from Documents into the data dir and into the ssh dir itself.
    let to_data = h.home.join("Documents/notes.txt");
    std::os::unix::fs::symlink(h.home.join("data/secret.db"), &to_data).expect("link");
    assert_eq!(list.check(&to_data), Err(PathDenied::Protected));
    let to_ssh = h.home.join("Documents/keys");
    std::os::unix::fs::symlink(h.home.join(".ssh"), &to_ssh).expect("link");
    assert_eq!(list.check(&to_ssh), Err(PathDenied::Protected));
    // A sibling that only shares a name prefix is not protected (component match).
    assert!(list.check(&h.home.join(".sshkeys/notes")).is_ok());
    // The ordinary file is allowed and comes back canonical.
    let doc = list
        .check(&h.home.join("Documents/a.txt"))
        .expect("allowed");
    assert_eq!(doc, h.home.join("Documents/a.txt").canonicalize().unwrap());
}

#[test]
fn uri_list_percent_encoded_traversal_is_refused_after_decoding() {
    let h = home_with_secrets();
    let list = denylist_for(&h);
    // %2E%2E is ".." after percent-decoding; the parser decodes, the check canonicalizes.
    let line = format!(
        "file://{}/Documents/%2E%2E/.ssh/id_ed25519\n",
        h.home.display()
    );
    let paths = parse_uri_list(line.as_bytes(), false);
    assert_eq!(paths.len(), 1);
    assert_eq!(list.check(&paths[0]), Err(PathDenied::Protected));
}
