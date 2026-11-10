use super::*;

fn mimes(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

pub(super) fn png(w: u32, h: u32) -> Vec<u8> {
    let mut d = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    d.extend_from_slice(b"IHDR");
    d.extend_from_slice(&w.to_be_bytes());
    d.extend_from_slice(&h.to_be_bytes());
    d.extend_from_slice(&[8, 6, 0, 0, 0]);
    d
}

#[test]
fn plan_prefers_utf8_text_and_needs_text_for_html() {
    let p = ReadPlan::from_mimes(&mimes(&[
        "text/html",
        "STRING",
        "UTF8_STRING",
        "text/plain",
    ]));
    assert_eq!(p.text.as_deref(), Some("UTF8_STRING"));
    assert_eq!(p.html.as_deref(), Some("text/html"));
    let only_html = ReadPlan::from_mimes(&mimes(&["text/html"]));
    assert!(only_html.html.is_none() && !only_html.wants_content());
    let png = ReadPlan::from_mimes(&mimes(&["image/png", "text/html"]));
    assert!(png.png.is_some() && png.html.is_none());
}

#[test]
fn uri_list_without_a_file_manager_type_is_not_wanted() {
    let bare = ReadPlan::from_mimes(&mimes(&["text/uri-list", "text/plain"]));
    assert!(!bare.wants_files() && bare.uri_list.is_some());
    let gnome = ReadPlan::from_mimes(&mimes(&["text/uri-list", "x-special/gnome-copied-files"]));
    assert!(gnome.wants_files());
    let kde = ReadPlan::from_mimes(&mimes(&["text/uri-list", "application/x-kde-cutselection"]));
    assert!(kde.wants_files());
    let fm_only = ReadPlan::from_mimes(&mimes(&["application/x-kde-cutselection"]));
    assert!(!fm_only.wants_files());
}

#[test]
fn reps_are_ordered_html_text_png_and_html_needs_text() {
    let reps = build_reps(RawReps {
        text: Some(b"hi".to_vec()),
        html: Some(b"<b>hi</b>".to_vec()),
        png: Some(png(10, 10)),
    })
    .expect("reps");
    let names: Vec<_> = reps.iter().map(|r| r.mime.as_str()).collect();
    assert_eq!(names, ["text/html", "text/plain", "image/png"]);
    let no_text = build_reps(RawReps {
        html: Some(b"<b>x</b>".to_vec()),
        ..Default::default()
    });
    assert_eq!(no_text.err(), Some(FormatSkip::NoContent));
}

#[test]
fn non_utf8_and_empty_text_is_dropped() {
    let r = build_reps(RawReps {
        text: Some(vec![0xff, 0xfe]),
        ..Default::default()
    });
    assert_eq!(r.err(), Some(FormatSkip::NoContent));
    let r = build_reps(RawReps {
        text: Some(Vec::new()),
        ..Default::default()
    });
    assert_eq!(r.err(), Some(FormatSkip::NoContent));
}

#[test]
fn png_bounds_and_signature() {
    assert!(png_within_bounds(&png(16384, 3906)));
    assert!(!png_within_bounds(&png(16385, 10)));
    assert!(!png_within_bounds(&png(16384, 16384)), "over 64 MP");
    assert!(!png_within_bounds(&png(0, 10)));
    assert!(!png_within_bounds(b"not a png at all, just text bytes"));
    assert!(!png_within_bounds(&[]));
    let r = build_reps(RawReps {
        png: Some(png(20000, 5)),
        ..Default::default()
    });
    assert_eq!(r.err(), Some(FormatSkip::BadImage));
}

#[test]
fn uri_list_parses_crlf_comments_percent_and_rejects_remote() {
    let data = b"# comment\r\nfile:///home/a/My%20File.txt\r\nfile://localhost/tmp/x\r\n\
                 file://otherhost/etc/passwd\r\nhttp://example.com/x\r\nfile:///a%00b\r\nfile:///bad%zz\r\n\r\n";
    let got = parse_uri_list(data, false);
    assert_eq!(
        got,
        vec![
            std::path::PathBuf::from("/home/a/My File.txt"),
            std::path::PathBuf::from("/tmp/x")
        ]
    );
}

#[test]
fn gnome_copied_files_skips_the_action_line() {
    let got = parse_uri_list(b"copy\nfile:///tmp/a\nfile:///tmp/b", true);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0], std::path::PathBuf::from("/tmp/a"));
}

#[test]
fn uri_list_percent_decodes_utf8_and_keeps_non_utf8_bytes() {
    let got = parse_uri_list(b"file:///tmp/%C5%82%C3%B3d%C5%BA", false);
    assert_eq!(got, vec![std::path::PathBuf::from("/tmp/łódź")]);
}

#[test]
fn entry_has_aliases_marker_and_hint_and_skips_unknown() {
    let reps = vec![
        ClipRep {
            mime: "text/plain".into(),
            data: b"hello".to_vec(),
        },
        ClipRep {
            mime: "text/html".into(),
            data: b"<i>hello</i>".to_vec(),
        },
        ClipRep {
            mime: "image/png".into(),
            data: png(4, 4),
        },
        ClipRep {
            mime: "text/rtf".into(),
            data: b"{\\rtf1}".to_vec(),
        },
    ];
    let e = to_entry(&reps);
    let names = e.offered_names();
    for n in TEXT_ALIASES
        .iter()
        .chain(&["text/html", "image/png", MARKER_MIME, KDE_HINT_MIME])
    {
        assert!(names.iter().any(|x| x == n), "missing {n}");
    }
    assert!(!names.iter().any(|n| n == "text/rtf"));
    assert_eq!(
        e.find("UTF8_STRING").map(|r| r.data.as_slice()),
        Some(&b"hello"[..])
    );
    assert_eq!(
        e.find(KDE_HINT_MIME).map(|r| r.data.as_slice()),
        Some(&b"secret"[..])
    );
}

#[test]
fn entry_without_writable_reps_is_empty() {
    assert!(
        to_entry(&[ClipRep {
            mime: "text/rtf".into(),
            data: vec![1]
        }])
        .reps
        .is_empty()
    );
    assert!(
        to_entry(&[ClipRep {
            mime: "image/png".into(),
            data: png(99999, 1)
        }])
        .reps
        .is_empty()
    );
    assert!(
        to_entry(&[ClipRep {
            mime: "text/plain".into(),
            data: vec![0xff]
        }])
        .reps
        .is_empty()
    );
}
