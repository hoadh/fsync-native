//! Engine-facing logic behind the Qt objects: start sequence, per-op threads, delegate,
//! secrets sync, IPC handling, tray, shutdown. Qt-free except through `core_state::hub`.

pub mod autostart_toggle;
pub mod boot;
mod boot_secrets;
pub mod clip_service_adapter;
pub mod clipboard_actions;
pub mod clipboard_runtime;
pub mod delegate_adapter;
#[cfg(feature = "e2e-control")]
pub mod e2e_control;
pub mod history_ops;
pub mod inbox_runtime;
pub mod ipc_handler;
pub mod offers;
mod offers_accept;
pub mod ops;
pub mod ops_account;
pub mod ops_devices;
pub mod ops_logout;
pub mod ops_pairing;
pub mod ops_pairing_events;
pub mod ops_recovery;
pub mod ops_session;
pub mod pending_revoke;
pub mod qr_image;
pub mod quick_clip_run;
pub mod secrets_sync;
pub mod send_files;
pub mod send_stage;
pub mod send_validate;
pub mod settings_ops;
pub mod shortcut_help;
pub mod shutdown;
pub mod startup;
pub mod tray;
pub mod user_messages;
pub mod xdg_open;
