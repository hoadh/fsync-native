use super::*;
use fsync_core::service::FileInfo;
use fsync_proto::ClipKind;
use std::cell::{Cell, RefCell};
use std::os::unix::fs::PermissionsExt;

/// Writes `rel_path` under the destination like core does, counting calls.
struct Fake {
    calls: Cell<u32>,
    rels: RefCell<Vec<String>>,
    fail_on: Option<u32>,
}

impl Fake {
    fn new(rels: &[&str]) -> Self {
        Self {
            calls: Cell::new(0),
            rels: RefCell::new(rels.iter().map(|s| (*s).to_owned()).collect()),
            fail_on: None,
        }
    }
}

impl FileFetcher for Fake {
    fn fetch_file(
        &self,
        _item: &str,
        file_id: u32,
        dest: &Path,
        _cancel: &dyn Fn() -> bool,
        _deadline: Instant,
    ) -> Result<PathBuf, String> {
        self.calls.set(self.calls.get() + 1);
        if self.fail_on == Some(file_id) {
            return Err("peer gone".into());
        }
        let rel = self.rels.borrow()[file_id as usize].clone();
        let p = dest.join(&rel);
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(&p, b"data").expect("write");
        Ok(p)
    }
}

fn item(id: &str, rels: &[&str]) -> ClipItem {
    ClipItem {
        item_id: id.into(),
        from: "n".into(),
        from_name: "peer".into(),
        kind: ClipKind::Files,
        ts_ms: 0,
        preview: None,
        reps: vec![],
        files: rels
            .iter()
            .enumerate()
            .map(|(i, r)| FileInfo {
                file_id: i as u32,
                name: (*r).into(),
                rel_path: (*r).into(),
                size: 4,
            })
            .collect(),
        total_size: 4 * rels.len() as u64,
        push: false,
    }
}

fn inbox() -> (tempfile::TempDir, Inbox) {
    let d = tempfile::tempdir().expect("tmp");
    let i = Inbox::new(d.path().join("inbox"));
    (d, i)
}

const ID: &str = "0b1c2d3e-aaaa-bbbb-cccc-0123456789ab";

#[test]
fn ids_and_components_are_validated() {
    assert!(valid_item_id(ID));
    for bad in [
        "",
        "../x",
        "a/b",
        "ABC",
        "x",
        &"a".repeat(65),
        "a b",
        "a\0b",
    ] {
        assert!(!valid_item_id(bad), "{bad:?}");
    }
    for ok in ["a.txt", "dir", ".hidden", "ü ß"] {
        assert!(valid_component(ok), "{ok:?}");
    }
    for bad in [
        "",
        ".",
        "..",
        "a/b",
        "a\0b",
        "a\nb",
        ".complete",
        &"x".repeat(256),
    ] {
        assert!(!valid_component(bad), "{bad:?}");
    }
}

#[test]
fn fetch_creates_a_private_dir_a_marker_and_top_level_entries() {
    let (_d, inbox) = inbox();
    let f = Fake::new(&["a.txt", "dir/x.txt", "dir/sub/y.txt"]);
    let got = inbox
        .fetch_item(
            &f,
            &item(ID, &["a.txt", "dir/x.txt", "dir/sub/y.txt"]),
            &|| false,
        )
        .expect("fetch");
    let names: Vec<_> = got
        .iter()
        .map(|p| {
            p.file_name()
                .expect("name")
                .to_str()
                .expect("utf8")
                .to_owned()
        })
        .collect();
    assert_eq!(names, ["a.txt", "dir"], "a folder is one entry");
    let dir = inbox.root().join(ID);
    assert_eq!(
        std::fs::metadata(&dir).expect("meta").permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(inbox.root())
            .expect("meta")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert!(
        got.iter()
            .all(|p| p.starts_with(std::fs::canonicalize(&dir).expect("canon")))
    );
}

#[test]
fn a_finished_copy_is_reused_without_fetching() {
    let (_d, inbox) = inbox();
    let f = Fake::new(&["a.txt"]);
    let first = inbox
        .fetch_item(&f, &item(ID, &["a.txt"]), &|| false)
        .expect("first");
    assert_eq!(f.calls.get(), 1);
    let second = inbox
        .fetch_item(&f, &item(ID, &["a.txt"]), &|| false)
        .expect("second");
    assert_eq!(f.calls.get(), 1, "reused");
    assert_eq!(first, second);
    // A damaged copy (entry removed) is fetched again.
    std::fs::remove_file(&first[0]).expect("rm");
    inbox
        .fetch_item(&f, &item(ID, &["a.txt"]), &|| false)
        .expect("third");
    assert_eq!(f.calls.get(), 2);
}

#[test]
fn unsafe_names_and_ids_are_refused_before_any_fetch() {
    let (_d, inbox) = inbox();
    for rels in [
        &["../evil"][..],
        &["a/../b"],
        &["/abs"],
        &["a//b"],
        &[".complete"],
        &["bad\0name"],
        &["x/"],
    ] {
        let f = Fake::new(&["x"]);
        let r = inbox.fetch_item(&f, &item(ID, rels), &|| false);
        assert!(matches!(r, Err(FetchError::Failed(_))), "{rels:?}");
        assert_eq!(f.calls.get(), 0);
    }
    let f = Fake::new(&["a"]);
    assert!(
        inbox
            .fetch_item(&f, &item("../etc", &["a"]), &|| false)
            .is_err()
    );
    assert!(inbox.fetch_item(&f, &item(ID, &[]), &|| false).is_err());
    assert!(!inbox.root().exists(), "nothing was created");
}

#[test]
fn cancel_and_failure_leave_no_partial_copy() {
    let (_d, inbox) = inbox();
    let f = Fake::new(&["a", "b"]);
    let n = Cell::new(0);
    let r = inbox.fetch_item(&f, &item(ID, &["a", "b"]), &|| {
        n.set(n.get() + 1);
        n.get() > 1
    });
    assert_eq!(r, Err(FetchError::Cancelled));
    assert!(!inbox.root().join(ID).exists());
    let mut f = Fake::new(&["a", "b"]);
    f.fail_on = Some(1);
    let r = inbox.fetch_item(&f, &item(ID, &["a", "b"]), &|| false);
    assert_eq!(r, Err(FetchError::Failed("peer gone".into())));
    assert!(!inbox.root().join(ID).exists());
    assert!(inbox.complete_entries(ID).is_none());
}

#[test]
fn a_partial_dir_is_replaced_and_a_symlinked_item_dir_is_not_followed() {
    let (d, inbox) = inbox();
    std::fs::create_dir_all(inbox.root().join(ID)).expect("mkdir");
    std::fs::write(inbox.root().join(ID).join("stale"), b"x").expect("stale");
    let f = Fake::new(&["a"]);
    inbox
        .fetch_item(&f, &item(ID, &["a"]), &|| false)
        .expect("fetch");
    assert!(!inbox.root().join(ID).join("stale").exists());

    let other = "0b1c2d3e-0000-0000-0000-000000000000";
    let victim = d.path().join("victim");
    std::fs::create_dir(&victim).expect("victim");
    std::fs::write(victim.join("keep"), b"k").expect("keep");
    std::os::unix::fs::symlink(&victim, inbox.root().join(other)).expect("link");
    assert!(inbox.complete_entries(other).is_none());
    inbox
        .fetch_item(&f, &item(other, &["a"]), &|| false)
        .expect("fetch over link");
    assert!(victim.join("keep").exists(), "the link target is untouched");
    assert!(!victim.join("a").exists());
}

/// A misbehaving fetcher that places the file next to the item dir (outside it).
struct Escaping;

impl FileFetcher for Escaping {
    fn fetch_file(
        &self,
        _item: &str,
        _file_id: u32,
        dest: &Path,
        _cancel: &dyn Fn() -> bool,
        _deadline: Instant,
    ) -> Result<PathBuf, String> {
        let p = dest.parent().expect("parent").join("escaped.txt");
        std::fs::write(&p, b"x").expect("write");
        Ok(p)
    }
}

#[test]
fn a_file_placed_outside_the_item_dir_fails_the_copy() {
    let (_d, inbox) = inbox();
    let r = inbox.fetch_item(&Escaping, &item(ID, &["a.txt"]), &|| false);
    assert_eq!(
        r,
        Err(FetchError::Failed(
            "A file was saved outside the inbox and was dropped.".into()
        ))
    );
    assert!(!inbox.root().join(ID).exists(), "no partial copy kept");
    assert!(inbox.complete_entries(ID).is_none());
}

#[test]
fn a_second_download_of_the_same_item_is_refused_until_the_first_ends() {
    let (_d, inbox) = inbox();
    let guard = inbox.begin_download(ID).expect("first claim");
    assert!(inbox.is_downloading(ID) && inbox.downloading() == [ID]);
    let f = Fake::new(&["a.txt"]);
    assert_eq!(
        inbox.fetch_item(&f, &item(ID, &["a.txt"]), &|| false),
        Err(FetchError::Failed("Already downloading.".into()))
    );
    assert_eq!(f.calls.get(), 0, "the second one fetched nothing");
    assert!(
        inbox.clone().begin_download(ID).is_err(),
        "clones share the set"
    );
    // The holder itself can download through its claim; the claim is released afterwards.
    inbox
        .fetch_item_guarded(&guard, &f, &item(ID, &["a.txt"]), &|| false, FETCH_BUDGET)
        .expect("guarded fetch");
    drop(guard);
    assert!(!inbox.is_downloading(ID));
    assert!(
        inbox
            .fetch_item(&f, &item(ID, &["a.txt"]), &|| false)
            .is_ok()
    );
    assert!(
        !inbox.is_downloading(ID),
        "released after a normal fetch too"
    );
    assert!(inbox.begin_download("../x").is_err());
}

#[test]
fn marker_names_are_reserved() {
    for n in [".complete", ".complete.tmp", ".explicit", ".explicit2"] {
        assert!(!valid_component(n), "{n}");
    }
    assert!(!valid_component(".completed-notes"));
    assert!(valid_component("complete"));
}
