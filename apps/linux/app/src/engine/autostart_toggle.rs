//! Settings: run at login (`$XDG_CONFIG_HOME/autostart/app.fsync.Fsync.desktop`).

use crate::core_state::Core;
use fsync_linux_platform::autostart;

pub fn is_enabled(core: &Core) -> bool {
    autostart::is_enabled(&core.paths.autostart_dir)
}

/// Writes or removes the entry; a failure is shown as a message, never fatal.
pub fn set(core: &Core, enabled: bool) {
    let result = if enabled {
        std::env::current_exe()
            .map_err(|e| e.to_string())
            .and_then(|exe| {
                autostart::enable(&core.paths.autostart_dir, &exe).map_err(|e| e.to_string())
            })
    } else {
        autostart::disable(&core.paths.autostart_dir).map_err(|e| e.to_string())
    };
    if let Err(e) = result {
        tracing::warn!("autostart: {e}");
        core.update_ui(|u| u.message = "Could not change the autostart setting.".into());
    }
}
