use super::*;
use std::fs;

struct Fixture {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    list: PathDenylist,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    for d in [
        ".ssh",
        ".gnupg",
        ".password-store",
        ".local/share/keyrings",
        ".config/fsync",
        "Documents",
        "data",
    ] {
        fs::create_dir_all(home.join(d)).expect("mkdir");
    }
    let list = PathDenylist::new(
        &home,
        &home.join("data"),
        &home.join(".local/share"),
        &home.join(".config"),
    );
    Fixture {
        home,
        list,
        _tmp: tmp,
    }
}

#[test]
fn ordinary_files_and_folders_are_allowed() {
    let f = fixture();
    let file = f.home.join("Documents/a.txt");
    fs::write(&file, b"x").expect("write");
    assert!(f.list.check(&file).is_ok());
    assert!(f.list.check(&f.home.join("Documents")).is_ok());
}

#[test]
fn protected_roots_and_their_children_are_denied() {
    let f = fixture();
    for d in [
        ".ssh",
        ".gnupg",
        ".password-store",
        ".local/share/keyrings",
        ".config/fsync",
        "data",
    ] {
        let dir = f.home.join(d);
        fs::write(dir.join("k"), b"x").expect("write");
        assert_eq!(
            f.list.check(&dir.join("k")),
            Err(PathDenied::Protected),
            "{d}"
        );
        assert_eq!(f.list.check(&dir), Err(PathDenied::Protected), "{d}");
    }
}

#[test]
fn symlink_into_a_protected_dir_is_denied_after_canonicalize() {
    let f = fixture();
    fs::write(f.home.join(".ssh/id_ed25519"), b"x").expect("write");
    let link = f.home.join("Documents/innocent.txt");
    std::os::unix::fs::symlink(f.home.join(".ssh/id_ed25519"), &link).expect("symlink");
    assert_eq!(f.list.check(&link), Err(PathDenied::Protected));
}

#[test]
fn a_folder_containing_a_protected_root_is_denied() {
    let f = fixture();
    assert_eq!(f.list.check(&f.home), Err(PathDenied::Protected));
}

#[test]
fn from_env_uses_the_xdg_dirs_when_set() {
    let f = fixture();
    let custom = f.home.join("cfg");
    std::fs::create_dir_all(custom.join("fsync")).expect("mkdir");
    let home = f.home.to_string_lossy().into_owned();
    let cfg = custom.to_string_lossy().into_owned();
    let env = move |k: &str| match k {
        "HOME" => Some(home.clone()),
        "XDG_CONFIG_HOME" => Some(cfg.clone()),
        _ => None,
    };
    let list = PathDenylist::from_env(&env, &f.home.join("data"));
    assert_eq!(
        list.check(&custom.join("fsync")),
        Err(PathDenied::Protected)
    );
    assert_eq!(list.check(&f.home.join(".ssh")), Err(PathDenied::Protected));
}

#[test]
fn more_credentials_and_system_paths_are_protected() {
    let f = fixture();
    for d in [
        ".aws",
        ".kube",
        ".docker",
        ".config/gh",
        ".mozilla",
        ".config/chromium",
    ] {
        let dir = f.home.join(d);
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join("k"), b"x").expect("write");
        assert_eq!(
            f.list.check(&dir.join("k")),
            Err(PathDenied::Protected),
            "{d}"
        );
    }
    fs::write(f.home.join(".netrc"), b"x").expect("write");
    assert_eq!(
        f.list.check(&f.home.join(".netrc")),
        Err(PathDenied::Protected)
    );
    for p in ["/proc/self/environ", "/sys", "/dev/null", "/etc/passwd"] {
        assert_eq!(
            f.list.check(Path::new(p)),
            Err(PathDenied::Protected),
            "{p}"
        );
    }
}

#[test]
fn tmp_outside_home_is_not_allowed_and_kwallet_is_protected() {
    let f = fixture();
    let outside = tempfile::tempdir().expect("tempdir"); // under /tmp, not under the fixture home
    let file = outside.path().join("a.txt");
    fs::write(&file, b"x").expect("write");
    assert_eq!(f.list.check(&file), Err(PathDenied::Protected));
    let kw = f.home.join(".local/share/kwalletd");
    fs::create_dir_all(&kw).expect("mkdir");
    fs::write(kw.join("kdewallet.kwl"), b"x").expect("write");
    assert_eq!(
        f.list.check(&kw.join("kdewallet.kwl")),
        Err(PathDenied::Protected)
    );
}

#[test]
fn unknown_home_fails_closed() {
    let none = |_: &str| None;
    let list = PathDenylist::from_env(&none, Path::new("/data"));
    assert_eq!(list.check(Path::new("/tmp")), Err(PathDenied::Protected));
    let rel = |k: &str| (k == "HOME").then(|| "relative".to_owned());
    let list = PathDenylist::from_env(&rel, Path::new("/data"));
    assert_eq!(list.check(Path::new("/tmp")), Err(PathDenied::Protected));
}

#[test]
fn missing_and_relative_paths_are_rejected() {
    let f = fixture();
    assert_eq!(
        f.list.check(&f.home.join("nope")),
        Err(PathDenied::Unresolvable)
    );
    assert_eq!(
        f.list.check(Path::new("relative/x")),
        Err(PathDenied::Unresolvable)
    );
}
