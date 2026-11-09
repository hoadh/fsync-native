//! Wire format: one JSON line per request, one JSON line reply. Validation lives here so the
//! server never hands unchecked data to the app.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[cfg(feature = "e2e-control")]
pub use super::protocol_e2e::E2eCommand;
#[cfg(feature = "e2e-control")]
use super::protocol_e2e::{fill_e2e, parse_e2e};
pub use super::protocol_reply::{Reply, read_line_bounded};

/// Largest accepted request/reply line, newline included.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
pub const MAX_PATHS: usize = 1000;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("request line too long")]
    TooLong,
    #[error("connection closed before a full line")]
    Truncated,
    #[error("io: {0}")]
    Io(String),
    #[error("invalid request: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcCommand {
    Show,
    ClipSend,
    ClipPaste,
    /// Absolute paths (syntax validated; the app checks they exist and are allowed).
    Send(Vec<PathBuf>),
    #[cfg(feature = "e2e-control")]
    E2e(E2eCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpcRequest {
    pub command: IpcCommand,
    /// Wayland xdg-activation token / X11 startup id of the launching process.
    pub activation_token: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
pub(super) struct Wire {
    pub(super) cmd: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) activation_token: Option<String>,
    // e2e-control fields; always accepted on the wire, only interpreted with the feature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) yes: Option<bool>,
}

pub(super) fn invalid(m: impl Into<String>) -> ProtocolError {
    ProtocolError::Invalid(m.into())
}

fn validate_paths(raw: Vec<String>) -> Result<Vec<PathBuf>, ProtocolError> {
    if raw.is_empty() {
        return Err(invalid("send needs at least one path"));
    }
    if raw.len() > MAX_PATHS {
        return Err(invalid(format!("more than {MAX_PATHS} paths")));
    }
    raw.into_iter()
        .map(|p| {
            let path = PathBuf::from(&p);
            if p.contains('\0') || !path.is_absolute() {
                return Err(invalid("paths must be absolute"));
            }
            // No existence check here: this runs on the IPC server thread, and a stat on a
            // hung mount would block every hotkey and the quit. The app resolves the paths
            // (and reports a missing one) on an op thread.
            Ok(path)
        })
        .collect()
}

impl IpcRequest {
    pub fn new(command: IpcCommand) -> Self {
        Self {
            command,
            activation_token: None,
        }
    }

    /// Fill `activation_token` from `XDG_ACTIVATION_TOKEN`, else `DESKTOP_STARTUP_ID`.
    pub fn with_activation_from_env(mut self, env: crate::paths::EnvFn<'_>) -> Self {
        if self.activation_token.is_none() {
            self.activation_token = ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"]
                .iter()
                .find_map(|k| env(k).filter(|v| !v.is_empty()));
        }
        self
    }

    /// Parse and validate one request line (server side).
    pub fn parse_line(line: &str) -> Result<Self, ProtocolError> {
        let w: Wire = serde_json::from_str(line.trim()).map_err(|e| invalid(e.to_string()))?;
        let command = match w.cmd.as_str() {
            "show" => IpcCommand::Show,
            "clip-send" => IpcCommand::ClipSend,
            "clip-paste" => IpcCommand::ClipPaste,
            "send" => IpcCommand::Send(validate_paths(w.paths)?),
            #[cfg(feature = "e2e-control")]
            other => IpcCommand::E2e(parse_e2e(other, &w)?),
            #[cfg(not(feature = "e2e-control"))]
            other => return Err(invalid(format!("unknown command {other:?}"))),
        };
        Ok(Self {
            command,
            activation_token: w.activation_token.filter(|t| !t.is_empty()),
        })
    }

    /// Serialize for sending (client side); relative `Send` paths are made absolute.
    pub fn to_line(&self) -> Result<String, ProtocolError> {
        let mut w = Wire {
            activation_token: self.activation_token.clone(),
            ..Wire::default()
        };
        match &self.command {
            IpcCommand::Show => w.cmd = "show".into(),
            IpcCommand::ClipSend => w.cmd = "clip-send".into(),
            IpcCommand::ClipPaste => w.cmd = "clip-paste".into(),
            IpcCommand::Send(paths) => {
                w.cmd = "send".into();
                for p in paths {
                    let abs =
                        std::path::absolute(p).map_err(|e| ProtocolError::Io(e.to_string()))?;
                    // A lossy copy would name a different file: refuse it here.
                    let text = abs.to_str().ok_or_else(|| invalid("path-unsupported"))?;
                    w.paths.push(text.to_owned());
                }
            }
            #[cfg(feature = "e2e-control")]
            IpcCommand::E2e(c) => fill_e2e(&mut w, c),
        }
        serde_json::to_string(&w).map_err(|e| invalid(e.to_string()))
    }
}
