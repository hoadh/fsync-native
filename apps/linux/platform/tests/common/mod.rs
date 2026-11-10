//! Headless display sessions for the clipboard integration tests: a private headless sway and an
//! Xvfb, each with its own socket, so the user's real clipboard is never touched. A missing tool
//! skips the test (prints why); the e2e script is stricter and exits 2.
#![allow(dead_code)]

use fsync_linux_platform::clipboard::{EntryRep, KDE_HINT_MIME, MARKER_MIME, OwnedEntry};

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub fn have(tool: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {tool} >/dev/null 2>&1"))
        .status()
        .is_ok_and(|s| s.success())
}

/// Returns false (after saying so) when a tool is missing.
pub fn need(tools: &[&str]) -> bool {
    let missing: Vec<_> = tools.iter().filter(|t| !have(t)).collect();
    if !missing.is_empty() {
        eprintln!("skipping: missing {missing:?}");
    }
    missing.is_empty()
}

pub struct Sway {
    child: Child,
    pub dir: tempfile::TempDir,
    pub socket: PathBuf,
}

impl Sway {
    pub fn start() -> Sway {
        let dir = tempfile::Builder::new()
            .prefix("fsync-hs-")
            .tempdir_in("/tmp")
            .expect("tempdir");
        std::fs::write(dir.path().join("sway.conf"), "default_border none\n").expect("conf");
        let child = Command::new("sway")
            .args(["-c"])
            .arg(dir.path().join("sway.conf"))
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("SWAYSOCK")
            .env_remove("HYPRLAND_INSTANCE_SIGNATURE")
            .env("XDG_RUNTIME_DIR", dir.path())
            .env("WLR_BACKENDS", "headless")
            .env("WLR_LIBINPUT_NO_DEVICES", "1")
            .env("WLR_RENDERER", "pixman")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sway");
        // Owned by the struct from here on: `Drop` reaps it on every path, including a panic.
        let mut sway = Sway {
            child,
            dir,
            socket: PathBuf::new(),
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let found = std::fs::read_dir(sway.dir.path())
                .expect("read_dir")
                .flatten()
                .map(|e| e.path())
                .find(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("wayland-") && !n.ends_with(".lock"))
                });
            if let Some(socket) = found {
                sway.socket = socket;
                return sway;
            }
            assert!(Instant::now() < deadline, "headless sway did not start");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn connection(&self) -> wayland_client::Connection {
        let stream = std::os::unix::net::UnixStream::connect(&self.socket).expect("connect sway");
        wayland_client::Connection::from_socket(stream).expect("wayland connection")
    }

    /// A wl-clipboard command bound to this compositor only.
    pub fn cmd(&self, program: &str) -> Command {
        let mut c = Command::new(program);
        c.env_remove("DISPLAY")
            .env("XDG_RUNTIME_DIR", self.dir.path())
            .env("WAYLAND_DISPLAY", self.socket.file_name().expect("name"));
        c
    }
}

impl Drop for Sway {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct Xvfb {
    child: Child,
    pub display: String,
    _dir: tempfile::TempDir,
}

impl Xvfb {
    pub fn start() -> Xvfb {
        let dir = tempfile::Builder::new()
            .prefix("fsync-xvfb-")
            .tempdir_in("/tmp")
            .expect("tempdir");
        // A display number far from the usual :0-:9, picked per test process; retried on a clash.
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        for _ in 0..20u32 {
            let attempt = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let num = 300 + (std::process::id() + attempt * 7) % 600;
            if Path::new(&format!("/tmp/.X{num}-lock")).exists() {
                continue;
            }
            let mut child = Command::new("Xvfb")
                .args([
                    &format!(":{num}"),
                    "-nolisten",
                    "tcp",
                    "-screen",
                    "0",
                    "800x600x24",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("Xvfb");
            let socket = PathBuf::from(format!("/tmp/.X11-unix/X{num}"));
            let deadline = Instant::now() + Duration::from_secs(10);
            while !socket.exists() && Instant::now() < deadline {
                if child.try_wait().ok().flatten().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            if socket.exists() {
                return Xvfb {
                    child,
                    display: format!(":{num}"),
                    _dir: dir,
                };
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        panic!("Xvfb did not start");
    }

    pub fn cmd(&self, program: &str) -> Command {
        let mut c = Command::new(program);
        c.env_remove("WAYLAND_DISPLAY")
            .env("DISPLAY", &self.display);
        c
    }
}

impl Drop for Xvfb {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // SIGKILL leaves the lock and the socket behind.
        let num = self.display.trim_start_matches(':');
        let _ = std::fs::remove_file(format!("/tmp/.X{num}-lock"));
        let _ = std::fs::remove_file(format!("/tmp/.X11-unix/X{num}"));
    }
}

/// Polls `f` until it returns `Some` (or panics after `secs`).
pub fn wait_for<T>(secs: u64, what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < deadline, "timeout waiting for {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// An entry like the app writes for a received text clip: aliases, marker and hint.
pub fn text_entry(text: &str) -> OwnedEntry {
    let aliases: Vec<String> = [
        "text/plain;charset=utf-8",
        "UTF8_STRING",
        "STRING",
        "TEXT",
        "text/plain",
    ]
    .iter()
    .map(|s| (*s).into())
    .collect();
    OwnedEntry {
        reps: vec![
            EntryRep {
                mime: "text/plain".into(),
                offered: aliases,
                data: text.into(),
            },
            EntryRep {
                mime: MARKER_MIME.into(),
                offered: vec![MARKER_MIME.into()],
                data: b"1".to_vec(),
            },
            EntryRep {
                mime: KDE_HINT_MIME.into(),
                offered: vec![KDE_HINT_MIME.into()],
                data: b"secret".to_vec(),
            },
        ],
    }
}

/// The next announced change, or a panic after 5 s.
pub fn next_change(
    rx: &std::sync::mpsc::Receiver<fsync_linux_platform::clipboard::ClipChange>,
) -> fsync_linux_platform::clipboard::ClipChange {
    rx.recv_timeout(Duration::from_secs(5))
        .expect("a clipboard change")
}

pub mod clip_fakes;
