//! XDG autostart entry: `$XDG_CONFIG_HOME/autostart/app.fsync.Fsync.desktop`.

use crate::APP_ID;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum AutostartError {
    #[error("executable path must be absolute and free of control characters")]
    BadExe,
    #[error("autostart io error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn entry_path(autostart_dir: &Path) -> PathBuf {
    autostart_dir.join(format!("{APP_ID}.desktop"))
}

/// Quote one argument per the desktop-entry spec: wrap in double quotes when it has reserved
/// characters; inside quotes escape `"`, `` ` ``, `$`, `\` with a backslash; then the string
/// escaping of the key value doubles every backslash and `%` becomes `%%`.
fn exec_quote(arg: &str) -> String {
    const RESERVED: &str = " \t\n\"'\\><~|&;$*?#()`";
    let mut quoted = String::new();
    for c in arg.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    if arg.chars().any(|c| RESERVED.contains(c)) {
        quoted = format!("\"{quoted}\"");
    }
    quoted.replace('\\', "\\\\").replace('%', "%%")
}

pub fn desktop_entry(exe: &Path) -> Result<String, AutostartError> {
    let exe_s = exe
        .to_str()
        .filter(|s| exe.is_absolute() && !s.chars().any(char::is_control));
    let exe_s = exe_s.ok_or(AutostartError::BadExe)?;
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=fsync\nComment=fsync clipboard and file sync\n\
         Exec={} --background\nIcon={APP_ID}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exec_quote(exe_s)
    ))
}

/// Write the entry (atomic tmp + rename), creating the autostart dir if needed.
pub fn enable(autostart_dir: &Path, exe: &Path) -> Result<(), AutostartError> {
    let content = desktop_entry(exe)?;
    std::fs::create_dir_all(autostart_dir)?;
    let target = entry_path(autostart_dir);
    let tmp = target.with_extension("desktop.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, &target).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(())
}

/// Remove the entry; absent is fine.
pub fn disable(autostart_dir: &Path) -> Result<(), AutostartError> {
    match std::fs::remove_file(entry_path(autostart_dir)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

pub fn is_enabled(autostart_dir: &Path) -> bool {
    entry_path(autostart_dir).is_file()
}
