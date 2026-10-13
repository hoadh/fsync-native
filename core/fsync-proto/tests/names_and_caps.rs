mod common;
use common::*;
use fsync_proto::{
    ClipKind, ProtoError, RepMeta,
    clip::{MAX_FILES, MAX_MIME_CHARS, MAX_REPS},
    names::{validate_device_name, validate_file_name, validate_rel_path},
};

#[test]
fn file_names_must_be_single_safe_components() {
    for ok in [
        "a.txt",
        "report (1).pdf",
        "日本語.txt",
        ".hidden",
        "CONSOLE",
        "com10",
        "a b",
    ] {
        assert_eq!(validate_file_name(ok), Ok(()), "{ok}");
    }
    let long = "x".repeat(256);
    for bad in [
        "",
        ".",
        "..",
        "a/b",
        "a\\b",
        "a:b",
        "a\0b",
        "a\nb",
        "a.",
        "a ",
        "CON",
        "con",
        "Nul.txt",
        "aux.tar.gz",
        "COM1",
        "lpt9.log",
        "PRN ",
        &long,
    ] {
        assert_eq!(
            validate_file_name(bad),
            Err(ProtoError::InvalidPath),
            "{bad:?}"
        );
    }
    assert_eq!(validate_file_name(&"x".repeat(255)), Ok(()));
}

#[test]
fn rel_paths_apply_component_rules_and_length_cap() {
    assert_eq!(validate_rel_path("dir/sub/file.txt"), Ok(()));
    for bad in [
        "",
        "/a",
        "a//b",
        "a/../b",
        "a/CON/b",
        "a/b.",
        "dir /f",
        "a/nul.txt",
        "a\\b",
    ] {
        assert_eq!(
            validate_rel_path(bad),
            Err(ProtoError::InvalidPath),
            "{bad:?}"
        );
    }
    let segs = vec!["abcdefgh"; 114].join("/"); // 114*8 + 113 = 1025 bytes
    assert_eq!(segs.len(), 1025);
    assert_eq!(validate_rel_path(&segs), Err(ProtoError::InvalidPath));
    assert_eq!(
        validate_rel_path(segs[..1024].trim_end_matches('/')),
        Ok(())
    );
}

#[test]
fn offer_rejects_bad_names_and_oversize_collections() {
    let with_files = |mut o: fsync_proto::ClipOffer, files| {
        o.kind = ClipKind::Files;
        o.files = files;
        o
    };
    let base = with_files(offer(), vec![file(1, "ok/a.txt")]);
    assert_eq!(base.validate(), Ok(()));

    let mut bad_name = base.clone();
    bad_name.files[0].name = "../x".into();
    assert_eq!(bad_name.validate(), Err(ProtoError::InvalidPath));
    bad_name.files[0].name = "NUL".into();
    assert_eq!(bad_name.validate(), Err(ProtoError::InvalidPath));

    let many = (0..=MAX_FILES as u32).map(|i| file(i, "a")).collect();
    assert!(matches!(
        with_files(offer(), many).validate(),
        Err(ProtoError::InvalidOffer("too many files"))
    ));
    let ok_many = (0..MAX_FILES as u32).map(|i| file(i, "a")).collect();
    assert_eq!(with_files(offer(), ok_many).validate(), Ok(()));

    let mut reps = offer();
    let rep = reps.reps[0].clone();
    reps.reps = vec![rep; MAX_REPS + 1];
    reps.inline = vec![None; MAX_REPS + 1];
    assert!(matches!(
        reps.validate(),
        Err(ProtoError::InvalidOffer("too many reps"))
    ));

    // `reps` and `inline` are zipped by consumers, so a mismatch must be rejected up front.
    let mut short = offer();
    short.inline.clear();
    assert!(matches!(
        short.validate(),
        Err(ProtoError::InvalidOffer("inline/reps length mismatch"))
    ));
    let mut long = offer();
    long.inline.push(None);
    assert!(matches!(
        long.validate(),
        Err(ProtoError::InvalidOffer("inline/reps length mismatch"))
    ));

    let mut mime = offer();
    mime.reps[0] = RepMeta {
        mime: "m".repeat(MAX_MIME_CHARS + 1),
        ..mime.reps[0].clone()
    };
    assert!(matches!(
        mime.validate(),
        Err(ProtoError::InvalidOffer("mime too long"))
    ));
}

#[test]
fn device_names_reject_control_bidi_and_bad_length() {
    for ok in ["MacBook", "Анна's PC", &"x".repeat(64)] {
        assert_eq!(validate_device_name(ok), Ok(()));
    }
    for bad in [
        "",
        "a\nb",
        "a\u{202E}b",
        "\u{200F}x",
        "a\u{2066}b",
        "bell\u{7}",
        &"x".repeat(65),
    ] {
        assert!(
            matches!(
                validate_device_name(bad),
                Err(ProtoError::InvalidDeviceList(_))
            ),
            "{bad:?}"
        );
    }
}

#[test]
fn windows_forbidden_chars_bidi_and_extra_device_names_are_rejected() {
    for bad in [
        "a<b",
        "a>b",
        "a\"b",
        "a|b",
        "a?b",
        "a*b.txt",
        "evil\u{202E}gpj.exe",
        "a\u{200B}b",
        "a\u{FEFF}b",
        "a\u{2066}b",
        "a\u{2028}b",
        "COM\u{B9}",
        "com\u{B2}.txt",
        "LPT\u{B3}",
        "CONIN$",
        "conout$.log",
    ] {
        assert_eq!(
            validate_file_name(bad),
            Err(ProtoError::InvalidPath),
            "{bad:?}"
        );
        assert!(validate_rel_path(&format!("dir/{bad}")).is_err(), "{bad:?}");
    }
    for ok in [
        "COM0x",
        "COM\u{B9}\u{B9}",
        "CONIN",
        "price $5.txt",
        "caf\u{E9}",
    ] {
        assert_eq!(validate_file_name(ok), Ok(()), "{ok:?}");
    }
    // Device names reject the invisible set too.
    assert!(validate_device_name("a\u{2028}b").is_err());
}
