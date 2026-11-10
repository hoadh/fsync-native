//! Test helper for the clipboard e2e: owns the clipboard with several MIME parts at once (wl-copy
//! and xclip can only offer one), through the same backends the app uses.
//!
//!   clip_offer --backend wayland|x11 [--display :N] [--socket PATH] MIME=FILE... [--hint]
//!
//! Prints `ready` once it owns the selection, then serves until it is killed or loses ownership
//! (exits with `lost`). Not part of the app.

use fsync_linux_platform::clipboard::wayland_backend::WaylandBackend;
use fsync_linux_platform::clipboard::x11_backend::X11Backend;
use fsync_linux_platform::clipboard::{Backend, EntryRep, KDE_HINT_MIME, OwnedEntry};
use std::sync::Arc;
use std::time::Duration;

fn fail(msg: &str) -> ! {
    eprintln!("clip_offer: {msg}");
    std::process::exit(2);
}

fn main() {
    let (mut backend, mut display, mut socket, mut hint) = (String::new(), None, None, false);
    let mut reps = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--backend" => {
                backend = args
                    .next()
                    .unwrap_or_else(|| fail("--backend needs a value"))
            }
            "--display" => display = args.next(),
            "--socket" => socket = args.next(),
            "--hint" => hint = true,
            part => {
                let (mime, file) = part
                    .split_once('=')
                    .unwrap_or_else(|| fail("MIME=FILE expected"));
                let data = std::fs::read(file).unwrap_or_else(|e| fail(&format!("{file}: {e}")));
                reps.push(EntryRep {
                    mime: mime.into(),
                    offered: vec![mime.into()],
                    data,
                });
            }
        }
    }
    if hint {
        reps.push(EntryRep {
            mime: KDE_HINT_MIME.into(),
            offered: vec![KDE_HINT_MIME.into()],
            data: b"secret".to_vec(),
        });
    }
    let backend: Arc<dyn Backend> = match backend.as_str() {
        "wayland" => {
            let path = socket.unwrap_or_else(|| fail("--socket needs the compositor socket"));
            let stream = std::os::unix::net::UnixStream::connect(path)
                .unwrap_or_else(|e| fail(&e.to_string()));
            let conn = wayland_client::Connection::from_socket(stream)
                .unwrap_or_else(|e| fail(&e.to_string()));
            WaylandBackend::start_on(conn).unwrap_or_else(|e| fail(&e))
        }
        "x11" => X11Backend::start(display).unwrap_or_else(|e| fail(&e)),
        _ => fail("--backend wayland|x11"),
    };
    backend
        .write(OwnedEntry { reps })
        .unwrap_or_else(|e| fail(&e.to_string()));
    println!("ready");
    while backend.owned() {
        std::thread::sleep(Duration::from_millis(100));
    }
    println!("lost");
    backend.shutdown();
}
