//! Black-box checks of the CLI surface that scripts and the P0 e2e demo rely on. No network:
//! only help output and argument parsing (clap usage errors exit 2) are exercised.

use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fsync-cli"))
        .args(args)
        .output()
        .expect("spawn fsync-cli")
}

#[test]
fn help_lists_the_p0_subcommands() {
    let out = cli(&["--help"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    for cmd in [
        "login", "pair", "devices", "clip", "send", "fetch", "history", "bench",
    ] {
        assert!(
            text.lines().any(|l| l.trim_start().starts_with(cmd)),
            "subcommand `{cmd}` missing from help:\n{text}"
        );
    }
}

#[test]
fn data_dir_is_required() {
    let out = cli(&["devices"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--data"));
}

#[test]
fn unknown_subcommand_is_a_usage_error() {
    let out = cli(&["--data", "/nonexistent-fsync-data", "nope"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn pair_requires_a_subcommand() {
    let out = cli(&["--data", "/nonexistent-fsync-data", "pair"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn clip_put_requires_text() {
    let out = cli(&["--data", "/nonexistent-fsync-data", "clip", "put"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--text"));
}

#[test]
fn history_reset_needs_yes_and_moves_the_history_aside() {
    let data = std::env::temp_dir().join(format!("fsync-cli-reset-{}", std::process::id()));
    let core = data.join("core");
    std::fs::create_dir_all(&core).unwrap();
    std::fs::write(core.join("history.db"), b"not really a database").unwrap();
    std::fs::write(core.join("history.keycheck"), b"tag").unwrap();
    let d = data.to_str().unwrap();

    let refused = cli(&["--data", d, "history", "reset"]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(
        core.join("history.db").exists(),
        "nothing touched without --yes"
    );

    let ok = cli(&["--data", d, "history", "reset", "--yes"]);
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    assert!(String::from_utf8_lossy(&ok.stdout).contains("history_reset"));
    assert!(!core.join("history.db").exists());
    assert!(!core.join("history.keycheck").exists());
    let kept = std::fs::read_dir(&core).unwrap().flatten().any(|e| {
        e.file_name()
            .to_string_lossy()
            .starts_with("history.corrupt-")
    });
    assert!(kept, "old history kept as a backup");
    let _ = std::fs::remove_dir_all(&data);
}
