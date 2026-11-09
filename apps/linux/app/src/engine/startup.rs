//! Process start sequence before Qt: paths, `app.lock` (held -> forward to the running instance),
//! config, secrets thread, core state, IPC server. The engine itself comes up in `boot`.

use super::ipc_handler;
use crate::cli_args::Args;
use crate::core_state::{self, Core, ui_state::EngineState};
use fsync_linux_platform::config_file::AppConfig;
use fsync_linux_platform::instance_lock::{InstanceLock, LockError};
use fsync_linux_platform::ipc::{self, IpcCommand, IpcRequest, Reply};
use fsync_linux_platform::paths::{Paths, data_dir_hash, process_env};
use fsync_linux_platform::secrets::{SecretsHandle, SystemProviders, file_override_allowed};
use std::sync::Arc;

/// Held for the process lifetime: dropping it releases `app.lock`.
pub struct Launch {
    pub core: Arc<Core>,
    pub _lock: InstanceLock,
}

fn command_of(args: &Args) -> IpcCommand {
    if args.clip_send {
        IpcCommand::ClipSend
    } else if args.clip_paste {
        IpcCommand::ClipPaste
    } else if !args.send.is_empty() {
        IpcCommand::Send(args.send.clone())
    } else {
        IpcCommand::Show
    }
}

/// Checked on the absolute form: a relative path under a non-UTF-8 cwd is not UTF-8 either.
fn send_paths_are_utf8(paths: &[std::path::PathBuf]) -> bool {
    paths
        .iter()
        .all(|p| std::path::absolute(p).is_ok_and(|a| a.to_str().is_some()))
}

/// Sends this launch's command to the running instance; returns the process exit code.
fn forward(paths: &Paths, data_dir: &std::path::Path, args: &Args) -> i32 {
    if !send_paths_are_utf8(&args.send) {
        eprintln!("fsync: path-unsupported");
        return 1;
    }
    let req = IpcRequest::new(command_of(args)).with_activation_from_env(&process_env);
    let hash = data_dir_hash(data_dir);
    match ipc::forward(&paths.runtime_dir, data_dir, &hash, &req) {
        Ok(Reply { ok: true, .. }) => 0,
        Ok(Reply { error, .. }) => {
            eprintln!("fsync: {}", error.unwrap_or_else(|| "refused".into()));
            1
        }
        Err(e) => {
            eprintln!("fsync: cannot reach the running instance: {e}");
            1
        }
    }
}

fn resolve_paths(args: &Args) -> Result<Paths, String> {
    let override_dir = match &args.data_dir {
        Some(d) => Some(std::path::absolute(d).map_err(|e| format!("--data-dir: {e}"))?),
        None => None,
    };
    let env = |k: &str| match (k, &override_dir) {
        ("FSYNC_DATA_DIR", Some(d)) => Some(d.to_string_lossy().into_owned()),
        _ => process_env(k),
    };
    Paths::resolve(&env).map_err(|e| e.to_string())
}

/// `Err(code)`: nothing more to do in this process (forwarded, or a startup error was printed).
pub fn prepare(args: &Args) -> Result<Launch, i32> {
    let fail = |m: String| {
        eprintln!("fsync: {m}");
        1
    };
    let paths = resolve_paths(args).map_err(fail)?;
    let data_dir = paths.ensure_data_dir().map_err(|e| fail(e.to_string()))?;
    // First thing: only the lock holder binds the socket or touches the engine.
    let lock = match InstanceLock::acquire(&data_dir) {
        Ok(l) => l,
        Err(LockError::Held) => return Err(forward(&paths, &data_dir, args)),
        Err(e) => return Err(fail(e.to_string())),
    };
    let cfg = AppConfig::load(&paths.config_file, &process_env).map_err(|e| fail(e.to_string()))?;
    let force_file =
        file_override_allowed() && std::env::var("FSYNC_SECRETS").is_ok_and(|v| v == "file");
    let secrets = SecretsHandle::spawn(
        data_dir.clone(),
        Arc::new(SystemProviders::default()),
        force_file,
    )
    .map_err(|e| fail(format!("secrets thread: {e}")))?;
    let core = core_state::install(Arc::new(Core::new(
        paths.clone(),
        data_dir.clone(),
        cfg,
        Some(secrets),
    )));
    super::secrets_sync::watch_saves(&core);
    start_ipc(&core, &lock, &data_dir);
    Ok(Launch { core, _lock: lock })
}

fn start_ipc(core: &Arc<Core>, lock: &InstanceLock, data_dir: &std::path::Path) {
    let handler_core = Arc::clone(core);
    let handler: Arc<ipc::server::Handler> =
        Arc::new(move |req| ipc_handler::handle(&handler_core, req));
    match ipc::IpcServer::start(
        &core.paths.runtime_dir,
        &data_dir_hash(data_dir),
        lock,
        handler,
    ) {
        Ok(server) => *crate::core_state::lock(&core.ipc) = Some(server),
        Err(e) => {
            tracing::warn!("ipc disabled: {e}");
            core.update_ui(|u| u.warnings.ipc_disabled = true);
        }
    }
}

/// Core for `--smoke`: QML loads against the real objects, but no secrets, no engine, no IPC.
pub fn inert_core() -> Arc<Core> {
    let dir = std::env::temp_dir().join("fsync-smoke");
    let paths = Paths {
        data_dir: dir.clone(),
        config_file: dir.join("config.toml"),
        autostart_dir: dir.join("autostart"),
        runtime_dir: dir.join("run"),
    };
    let cfg = AppConfig {
        api: "http://127.0.0.1:9".into(),
        relays: Vec::new(),
        device_name: "smoke".into(),
        allow_insecure_http: true,
    };
    let core = Core::new(paths, dir, cfg, None);
    core.update_ui(|u| u.state = EngineState::Starting);
    core_state::install(Arc::new(core))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    #[test]
    fn utf8_check_covers_the_absolute_form() {
        assert!(send_paths_are_utf8(&[
            PathBuf::from("/tmp/a"),
            PathBuf::from("rel/b")
        ]));
        let odd = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/bad-\xff".to_vec()));
        assert!(!send_paths_are_utf8(&[odd]));
        // A relative path becomes absolute against the cwd: a non-UTF-8 cwd makes it non-UTF-8.
        let old = std::env::current_dir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let bad = dir
            .path()
            .join(std::ffi::OsString::from_vec(b"cwd-\xff".to_vec()));
        std::fs::create_dir(&bad).unwrap();
        std::env::set_current_dir(&bad).unwrap();
        let ok = send_paths_are_utf8(&[PathBuf::from("rel")]);
        std::env::set_current_dir(old).unwrap();
        assert!(!ok);
    }
}
