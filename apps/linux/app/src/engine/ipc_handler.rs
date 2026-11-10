//! Commands from a second launch (and, in test builds, the e2e controller). Runs on the IPC
//! server thread: every branch answers fast, long work goes to op threads.

use super::clipboard_actions::{Action, request};
use super::send_files;
use crate::core_state::{Core, lock};
use fsync_linux_platform::ipc::{IpcCommand, IpcRequest, Reply};
use std::sync::Arc;

fn clip(core: &Arc<Core>, action: Action, token: Option<String>) -> Reply {
    match request(core, action, token) {
        Ok(()) => Reply::ok(),
        Err(code) => Reply::err(code),
    }
}

pub fn handle(core: &Arc<Core>, req: IpcRequest) -> Reply {
    if !core.gate.is_open() {
        return Reply::err("shutting-down");
    }
    match req.command {
        IpcCommand::Show => {
            core.hub.show_window(req.activation_token);
            Reply::ok()
        }
        // No queue for clipboard commands: acting on a stale hotkey press would surprise.
        IpcCommand::ClipSend => clip(core, Action::Send, req.activation_token),
        IpcCommand::ClipPaste => clip(core, Action::Paste, req.activation_token),
        // Staged and checked on an op thread (the IPC thread never touches the file system):
        // the verdict is awaited for 2 s at most, then "queued" and the result shows in the
        // window. Answered by the DevicePicker; never queued across a sign-in.
        IpcCommand::Send(paths) => {
            if !lock(&core.ui).state.is_ready() {
                core.hub.show_window(req.activation_token);
                return Reply::err("signed-out");
            }
            match send_files::stage_bounded(core, paths, req.activation_token) {
                Ok(()) => Reply::ok(),
                Err(code) => Reply::err(code),
            }
        }
        #[cfg(feature = "e2e-control")]
        IpcCommand::E2e(cmd) => super::e2e_control::handle(core, cmd),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_state::ui_state::EngineState;
    use fsync_linux_platform::config_file::AppConfig;
    use fsync_linux_platform::paths::Paths;

    fn core() -> Arc<Core> {
        let dir = std::path::PathBuf::from("/nonexistent/fsync-test");
        let paths = Paths {
            data_dir: dir.clone(),
            config_file: dir.join("config.toml"),
            autostart_dir: dir.join("autostart"),
            runtime_dir: dir.join("run"),
        };
        let cfg = AppConfig {
            api: "http://127.0.0.1:9".into(),
            relays: Vec::new(),
            device_name: "t".into(),
            allow_insecure_http: true,
        };
        Arc::new(Core::new(paths, dir, cfg, None))
    }

    fn reply(core: &Arc<Core>, cmd: IpcCommand) -> Reply {
        handle(core, IpcRequest::new(cmd))
    }

    #[test]
    fn clip_commands_answer_signed_out_until_ready() {
        let c = core();
        for state in [
            EngineState::Starting,
            EngineState::SignedOut,
            EngineState::Offline,
        ] {
            c.update_ui(|u| u.state = state);
            assert_eq!(
                reply(&c, IpcCommand::ClipSend).error.as_deref(),
                Some("signed-out")
            );
            assert_eq!(
                reply(&c, IpcCommand::ClipPaste).error.as_deref(),
                Some("signed-out")
            );
        }
        // Ready, but the clipboard engine is not up (boot has not reached it): never queued.
        c.update_ui(|u| u.state = EngineState::Ready);
        for cmd in [IpcCommand::ClipSend, IpcCommand::ClipPaste] {
            assert_eq!(reply(&c, cmd).error.as_deref(), Some("not-ready"));
        }
    }

    #[test]
    fn send_is_refused_until_ready_and_never_queued() {
        let c = core();
        let send = || IpcCommand::Send(vec![std::path::PathBuf::from("/etc/hostname")]);
        for state in [EngineState::Starting, EngineState::SignedOut] {
            c.update_ui(|u| u.state = state);
            assert_eq!(reply(&c, send()).error.as_deref(), Some("signed-out"));
        }
        // Ready: a path outside HOME / removable media is refused by the allowlist, not staged.
        c.update_ui(|u| u.state = EngineState::Ready);
        assert_eq!(
            reply(
                &c,
                IpcCommand::Send(vec![std::path::PathBuf::from("/etc/hostname")])
            )
            .error
            .as_deref(),
            Some("path-protected")
        );
        assert!(lock(&c.pending_send).is_empty());
    }

    #[test]
    fn show_is_ok_and_everything_is_refused_after_the_gate_closes() {
        let c = core();
        assert!(reply(&c, IpcCommand::Show).ok);
        c.gate.close();
        assert_eq!(
            reply(&c, IpcCommand::Show).error.as_deref(),
            Some("shutting-down")
        );
    }
}
