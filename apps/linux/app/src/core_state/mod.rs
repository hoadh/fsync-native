//! Process-wide state shared by the QML-created objects (which cannot take constructor
//! arguments) and the worker threads. Installed once in `main` before QML loads.

pub mod boot_control;
pub mod files_state;
pub mod gate;
pub mod hub;
pub mod in_flight;
pub mod offers_state;
pub mod staged_send;
pub mod ui_state;

use boot_control::BootControl;
use files_state::FilesState;
use fsync_core::service::FsyncService;
use fsync_linux_platform::config_file::AppConfig;
use fsync_linux_platform::files::inbox::Inbox;
use fsync_linux_platform::files::transfer_wait::TransferWaiter;
use fsync_linux_platform::ipc::IpcServer;
use fsync_linux_platform::paths::Paths;
use fsync_linux_platform::secrets::{Secrets, SecretsHandle};
use fsync_linux_platform::sync::SyncEngine;
use gate::Gate;
use hub::Hub;
use in_flight::InFlight;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock};
use ui_state::{DeviceRow, PairingUi, UiState};

pub struct Core {
    pub paths: Paths,
    /// Canonical data dir (hash input for the IPC socket and keyring attributes).
    pub data_dir: PathBuf,
    pub cfg: AppConfig,
    /// `None` in `--smoke`: nothing touches secrets there.
    pub secrets: Option<SecretsHandle>,
    /// What the secrets store holds, mirrored so partial updates can be saved as a whole.
    pub secrets_mem: Mutex<Option<Secrets>>,
    /// Serialises "update mirror + save" so saves reach the secrets thread in update order.
    pub persist_lock: Mutex<()>,
    /// Highest save sequence whose caller saw a failure; the "not saved" banner stays until a
    /// save at or after it has been applied (see `engine::secrets_sync`).
    pub failed_save_seq: AtomicU64,
    pub boot: BootControl,
    /// Ops that may start the engine (see `in_flight`).
    pub starting: InFlight,
    service: RwLock<Option<Arc<FsyncService>>>,
    pub ui: Mutex<UiState>,
    pub pairing: Mutex<PairingUi>,
    pub devices: Mutex<Vec<DeviceRow>>,
    pub gate: Gate,
    pub hub: Hub,
    /// Set by the tray thread while a StatusNotifierWatcher hosts our icon.
    pub tray_registered: Arc<AtomicBool>,
    /// Bound only by the `app.lock` holder; taken (and stopped) at shutdown.
    pub ipc: Mutex<Option<IpcServer>>,
    /// Clipboard sync (started once the service exists, taken and stopped at shutdown).
    pub clip: Mutex<Option<Arc<SyncEngine>>>,
    /// The first-run privacy notice is still to be shown.
    pub clip_notice: AtomicBool,
    /// End of every core receive transfer, for the workers that wait for one.
    pub transfers: TransferWaiter,
    /// Offers, history rows and held large file clips (phase 04a).
    pub files: FilesState,
    /// The one inbox of received file clips (its in-flight set is shared by every download).
    pub inbox: Inbox,
    /// Files of a `--send` / window drop that wait for the DevicePicker answer.
    pub pending_send: Mutex<staged_send::Staged>,
    /// Stage requests being checked on op threads (bounded: a dead mount blocks them).
    pub stage_inflight: std::sync::atomic::AtomicUsize,
}

static CORE: OnceLock<Arc<Core>> = OnceLock::new();

/// Poison-tolerant lock: a panicked worker must not freeze the UI.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Core {
    pub fn new(
        paths: Paths,
        data_dir: PathBuf,
        cfg: AppConfig,
        secrets: Option<SecretsHandle>,
    ) -> Self {
        let data_dir_for_inbox = data_dir.join("inbox");
        Self {
            paths,
            data_dir,
            cfg,
            secrets,
            secrets_mem: Mutex::new(None),
            persist_lock: Mutex::new(()),
            failed_save_seq: AtomicU64::new(0),
            boot: BootControl::default(),
            starting: InFlight::default(),
            service: RwLock::new(None),
            ui: Mutex::new(UiState::default()),
            pairing: Mutex::new(PairingUi::default()),
            devices: Mutex::new(Vec::new()),
            gate: Gate::default(),
            hub: Hub::default(),
            tray_registered: Arc::new(AtomicBool::new(false)),
            ipc: Mutex::new(None),
            clip: Mutex::new(None),
            clip_notice: AtomicBool::new(false),
            transfers: TransferWaiter::default(),
            files: FilesState::default(),
            inbox: Inbox::new(data_dir_for_inbox),
            pending_send: Mutex::new(staged_send::Staged::default()),
            stage_inflight: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn service(&self) -> Option<Arc<FsyncService>> {
        self.service
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn set_service(&self, svc: Option<Arc<FsyncService>>) {
        *self.service.write().unwrap_or_else(|p| p.into_inner()) = svc;
    }

    /// Changes the UI snapshot and tells the engine object to re-read it.
    pub fn update_ui(&self, f: impl FnOnce(&mut UiState)) {
        f(&mut lock(&self.ui));
        self.hub.poke_engine();
    }

    pub fn update_pairing(&self, f: impl FnOnce(&mut PairingUi)) {
        f(&mut lock(&self.pairing));
        self.hub.poke_pairing();
    }

    pub fn set_devices(&self, rows: Vec<DeviceRow>) {
        *lock(&self.devices) = rows;
        self.hub.poke_devices();
    }
}

/// Installs the process-wide core; a second call is ignored (first wins).
pub fn install(core: Arc<Core>) -> Arc<Core> {
    CORE.get_or_init(|| core).clone()
}

pub fn get() -> Option<Arc<Core>> {
    CORE.get().cloned()
}
