//! Plain snapshots of what the UI shows. Worker threads mutate them under `Core`'s mutexes and
//! poke the Qt objects, which copy the snapshot into their properties on the GUI thread. A snapshot
//! (rather than queued per-field closures) means an object created late still sees current state.

use fsync_core::service::{Device, DeviceStatus};
use fsync_proto::Platform;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EngineState {
    #[default]
    Starting,
    WaitingForKeyring,
    SignedOut,
    AwaitingCode,
    /// Signed in, backend unreachable: retry offered.
    Offline,
    /// Signed in, this device is not in the account's device list yet.
    NeedsPairing,
    Ready,
    Error,
}

impl EngineState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::WaitingForKeyring => "waiting-for-keyring",
            Self::SignedOut => "signed-out",
            Self::AwaitingCode => "awaiting-code",
            Self::Offline => "offline",
            Self::NeedsPairing => "needs-pairing",
            Self::Ready => "ready",
            Self::Error => "error",
        }
    }

    /// Signed in to an account (the device list is meaningful).
    pub fn is_signed_in(self) -> bool {
        matches!(self, Self::Ready | Self::NeedsPairing | Self::Offline)
    }

    /// Clipboard commands are served only in these states (phase 03).
    pub fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// Persistent warnings; each one is a sentence the banner shows.
#[derive(Clone, Debug, Default)]
pub struct Warnings {
    pub file_fallback: bool,
    pub secrets_unsaved: bool,
    pub ipc_disabled: bool,
    pub waiting_keyring: bool,
    /// The Secret Service backend owns this data dir (an "Unlock keyring" action makes sense).
    pub keyring_backend: bool,
}

impl Warnings {
    /// Whether the banner offers "Unlock keyring".
    pub fn can_unlock(&self) -> bool {
        self.waiting_keyring || (self.secrets_unsaved && self.keyring_backend)
    }

    /// `booting`: the engine is still starting (waiting for the keyring is then a wait, not a
    /// runtime failure).
    pub fn banner(&self, booting: bool) -> String {
        let mut parts: Vec<&str> = Vec::new();
        let runtime_lock = self.waiting_keyring && !booting;
        if runtime_lock {
            parts.push("Keyring locked: changes are kept in memory.");
        } else if self.waiting_keyring {
            parts.push("Waiting for the system keyring. Unlock it to continue.");
        }
        if self.file_fallback {
            parts.push("No keyring found: secrets are stored in a protected file.");
        }
        // The runtime-lock sentence already says it.
        if self.secrets_unsaved && !runtime_lock {
            parts.push("Secrets could not be saved; they are kept in memory only.");
        }
        if self.ipc_disabled {
            parts.push("Single-instance control is unavailable.");
        }
        parts.join(" ")
    }
}

#[derive(Clone, Debug, Default)]
pub struct UiState {
    pub state: EngineState,
    pub email: String,
    pub node_id: String,
    pub warnings: Warnings,
    /// Last user-facing problem of an action; cleared by the next one.
    pub message: String,
    pub busy: bool,
    /// Boot failed with `HistoryKeyMismatch`: the Error page offers "Reset history".
    pub history_mismatch: bool,
    /// `(reset_id, device names)` of a trust reset waiting for an answer.
    pub trust_reset: Option<(String, Vec<String>)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PairingPhase {
    #[default]
    Idle,
    /// Offer open, waiting for the other device.
    Offering,
    Joining,
    Confirm,
    /// SAS answered; waiting for the other device to finish.
    Answering,
    Done,
    Failed,
}

impl PairingPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Offering => "offering",
            Self::Joining => "joining",
            Self::Confirm => "confirm",
            Self::Answering => "answering",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PairingUi {
    pub phase: PairingPhase,
    pub code: String,
    pub qr_url: String,
    /// The `fsync://pair?...` text behind the QR image (for devices that cannot scan).
    pub qr_text: String,
    pub sas: String,
    pub peer_name: String,
    pub request_id: u64,
    pub message: String,
    /// Stray confirmations rejected while Answering/Joining: core answers each with a `Failed`
    /// event that must not fail the pairing in flight. Cleared with every other phase change.
    pub stray_rejects: u32,
}

#[derive(Clone, Debug)]
pub struct DeviceRow {
    pub node_id: String,
    pub name: String,
    pub platform: &'static str,
    pub status: &'static str,
    pub is_self: bool,
}

pub fn platform_label(p: Platform) -> &'static str {
    // Linux and unknown platforms read as Cli through `wire_compat`, like every other client.
    match p.wire_compat() {
        Platform::Mac => "macOS",
        Platform::Win => "Windows",
        Platform::Cli | Platform::Linux | Platform::Unknown(_) => "Command line",
    }
}

impl From<&Device> for DeviceRow {
    fn from(d: &Device) -> Self {
        Self {
            node_id: d.node_id.clone(),
            name: d.name.clone(),
            platform: platform_label(d.platform),
            status: match d.status {
                DeviceStatus::Lan => "Lan",
                DeviceStatus::Relay => "Relay",
                DeviceStatus::Offline => "Offline",
            },
            is_self: d.is_self,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_joins_active_warnings_only() {
        let mut w = Warnings::default();
        assert_eq!(w.banner(true), "");
        w.file_fallback = true;
        w.secrets_unsaved = true;
        assert!(
            w.banner(false).contains("protected file") && w.banner(false).contains("memory only")
        );
    }

    #[test]
    fn keyring_text_differs_between_boot_and_runtime() {
        let w = Warnings {
            waiting_keyring: true,
            secrets_unsaved: true,
            ..Warnings::default()
        };
        assert!(w.banner(true).starts_with("Waiting for the system keyring"));
        assert_eq!(
            w.banner(false),
            "Keyring locked: changes are kept in memory."
        );
    }

    #[test]
    fn linux_and_unknown_platforms_show_as_cli() {
        assert_eq!(platform_label(Platform::Unknown(9)), "Command line");
        assert_eq!(platform_label(Platform::Mac), "macOS");
    }
}
