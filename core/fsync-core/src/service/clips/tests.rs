use super::*;
use crate::service::types::ClipRep;

fn clip(mime: &str, data: &[u8]) -> LocalClip {
    LocalClip {
        reps: vec![ClipRep {
            mime: mime.into(),
            data: data.to_vec(),
        }],
        files: vec![],
        preview: None,
        sensitive: false,
    }
}

#[test]
fn kinds_follow_first_rep_or_files() {
    assert_eq!(kind_of(&clip("text/plain", b"x")).unwrap(), ClipKind::Text);
    assert_eq!(kind_of(&clip("image/png", b"x")).unwrap(), ClipKind::Image);
    assert_eq!(kind_of(&clip("text/html", b"x")).unwrap(), ClipKind::Rich);
    let mut f = clip("text/plain", b"x");
    f.files.push("/a".into());
    assert_eq!(kind_of(&f).unwrap(), ClipKind::Files);
    assert!(
        kind_of(&LocalClip {
            reps: vec![],
            ..clip("a", b"")
        })
        .is_err()
    );
}

#[test]
fn preview_is_cut_and_only_for_text() {
    let long = "é".repeat(500);
    let p = preview_of(&clip("text/plain", long.as_bytes()), ClipKind::Text).unwrap();
    assert_eq!(p.chars().count(), 200);
    assert!(preview_of(&clip("image/png", b"\x89PNG"), ClipKind::Image).is_none());
}
