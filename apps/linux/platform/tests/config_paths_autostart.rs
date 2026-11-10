use fsync_linux_platform::autostart;
use fsync_linux_platform::config_file::{AppConfig, DEFAULT_API, DEFAULT_RELAYS, is_loopback_http};
use fsync_linux_platform::paths::{Paths, create_private_dir, data_dir_hash};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let m: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |k| m.get(k).cloned()
}

#[test]
fn paths_follow_xdg_and_overrides_and_never_use_tmp() {
    let p = Paths::resolve(&env_of(&[("HOME", "/h")])).unwrap();
    assert_eq!(p.data_dir, Path::new("/h/.local/share/fsync"));
    assert_eq!(p.config_file, Path::new("/h/.config/fsync/config.toml"));
    assert_eq!(p.runtime_dir, Path::new("/h/.local/share/fsync/run"));
    let p = Paths::resolve(&env_of(&[
        ("HOME", "/h"),
        ("XDG_DATA_HOME", "/xd"),
        ("XDG_CONFIG_HOME", "/xc"),
        ("XDG_RUNTIME_DIR", "/run/user/1"),
    ]))
    .unwrap();
    assert_eq!(p.data_dir, Path::new("/xd/fsync"));
    assert_eq!(p.config_file, Path::new("/xc/fsync/config.toml"));
    assert_eq!(p.autostart_dir, Path::new("/xc/autostart"));
    assert_eq!(p.runtime_dir, Path::new("/run/user/1/fsync"));
    let p = Paths::resolve(&env_of(&[("FSYNC_DATA_DIR", "/data"), ("HOME", "/h")])).unwrap();
    assert_eq!(p.data_dir, Path::new("/data"));
    assert!(Paths::resolve(&env_of(&[])).is_err());
    assert!(Paths::resolve(&env_of(&[("HOME", "/h"), ("FSYNC_DATA_DIR", "rel")])).is_err());
}

#[test]
fn data_dir_created_private_and_hash_is_stable() {
    let d = tempfile::tempdir().unwrap();
    let p = Paths::resolve(&env_of(&[
        ("HOME", "/h"),
        ("FSYNC_DATA_DIR", d.path().join("a/b").to_str().unwrap()),
    ]))
    .unwrap();
    let canon = p.ensure_data_dir().unwrap();
    assert_eq!(
        std::fs::metadata(&canon).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(data_dir_hash(&canon), data_dir_hash(&canon));
    assert_eq!(data_dir_hash(&canon).len(), 64);
    assert_ne!(data_dir_hash(&canon), data_dir_hash(d.path()));
    create_private_dir(&canon).unwrap();
}

#[test]
fn config_defaults_file_and_env_precedence() {
    let d = tempfile::tempdir().unwrap();
    let cfg = d.path().join("config.toml");
    let c = AppConfig::load(&cfg, &env_of(&[])).unwrap();
    assert_eq!(c.api, DEFAULT_API);
    assert_eq!(c.relays, DEFAULT_RELAYS);
    assert!(!c.allow_insecure_http);
    assert!(!c.device_name.is_empty());

    std::fs::write(
        &cfg,
        "api = \"https://x.example\"\nrelays = [\"https://r1\", \"https://r2\"]\n",
    )
    .unwrap();
    let c = AppConfig::load(&cfg, &env_of(&[])).unwrap();
    assert_eq!((c.api.as_str(), c.relays.len()), ("https://x.example", 2));

    let c = AppConfig::load(
        &cfg,
        &env_of(&[
            ("FSYNC_API", "http://127.0.0.1:8080"),
            ("FSYNC_RELAY", " http://a:1 , ,http://b:2"),
            ("FSYNC_DEVICE_NAME", "box"),
        ]),
    )
    .unwrap();
    assert_eq!(c.relays, ["http://a:1", "http://b:2"]);
    assert_eq!(c.device_name, "box");
    assert!(c.allow_insecure_http);

    std::fs::write(&cfg, "api = [").unwrap();
    assert!(AppConfig::load(&cfg, &env_of(&[])).is_err());
    std::fs::write(&cfg, "bogus = 1").unwrap();
    assert!(AppConfig::load(&cfg, &env_of(&[])).is_err());
}

#[test]
fn insecure_http_only_for_loopback() {
    for ok in [
        "http://127.0.0.1:8080",
        "http://localhost",
        "http://[::1]:1/x",
        "http://127.1.2.3",
    ] {
        assert!(is_loopback_http(ok), "{ok}");
    }
    for bad in [
        "https://127.0.0.1",
        "http://example.com",
        "http://127.0.0.1@evil.com",
        "http://localhost.evil.com",
    ] {
        assert!(!is_loopback_http(bad), "{bad}");
    }
}

#[test]
fn autostart_write_quote_and_remove() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("autostart");
    assert!(!autostart::is_enabled(&dir));
    autostart::enable(&dir, Path::new("/opt/my app/fsync-linux")).unwrap();
    let text = std::fs::read_to_string(autostart::entry_path(&dir)).unwrap();
    assert!(dir.join("app.fsync.Fsync.desktop").is_file());
    assert!(
        text.contains("Exec=\"/opt/my app/fsync-linux\" --background\n"),
        "{text}"
    );
    assert!(autostart::is_enabled(&dir));
    autostart::enable(&dir, Path::new("/usr/bin/fsync-linux")).unwrap();
    assert!(
        std::fs::read_to_string(autostart::entry_path(&dir))
            .unwrap()
            .contains("Exec=/usr/bin/fsync-linux --background")
    );
    autostart::disable(&dir).unwrap();
    autostart::disable(&dir).unwrap();
    assert!(!autostart::is_enabled(&dir));
    assert!(autostart::enable(&dir, Path::new("relative/exe")).is_err());
}
