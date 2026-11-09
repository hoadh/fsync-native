//! Send / paste requests from the tray, the IPC (`--clip-send`, `--clip-paste`) and the status
//! page. With a background backend they act directly; on a Manual backend (GNOME) they open the
//! QuickClipWindow, which needs the launcher's activation token to get the focus.

use super::clipboard_runtime::engine;
use crate::core_state::{Core, lock};
use fsync_linux_platform::sync::Paste;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Send,
    Paste,
}

impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Paste => "paste",
        }
    }
}

/// `Err` is a short machine code (IPC reply `error`).
pub fn request(
    core: &Arc<Core>,
    action: Action,
    activation_token: Option<String>,
) -> Result<(), &'static str> {
    if !lock(&core.ui).state.is_ready() {
        return Err("signed-out");
    }
    let engine = engine(core).ok_or("not-ready")?;
    if !engine.has_backend() {
        return if core.hub.quick_clip(action.name(), activation_token) {
            Ok(())
        } else {
            Err("not-ready")
        };
    }
    match action {
        Action::Send if engine.send_now() => Ok(()),
        Action::Send => Err("nothing-to-send"),
        Action::Paste => {
            if engine.status().pending.is_none() {
                return Err("nothing-to-paste");
            }
            // The backend write can take up to 2 s: never on the IPC or tray thread.
            let spawned = std::thread::Builder::new()
                .name("fsync-clip-paste".into())
                .spawn(move || {
                    if let Paste::Failed(e) = engine.paste() {
                        tracing::warn!("clipboard paste failed: {e}");
                    }
                });
            spawned.map(|_| ()).map_err(|_| "write-failed")
        }
    }
}
