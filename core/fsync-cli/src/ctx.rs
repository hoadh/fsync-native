//! Opens the service for a command and offers the shared wait helpers.

use crate::{
    args::Global,
    events::{CliDelegate, Ev},
    session,
};
use anyhow::{Result, bail};
use fsync_core::{
    Identity,
    service::{AccountState, Device, FsyncService, ServiceConfig},
};
use fsync_proto::Platform;
use std::{
    io::Write,
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{Receiver, RecvTimeoutError, channel},
    },
    time::{Duration, Instant},
};

pub struct Ctx {
    pub svc: FsyncService,
    pub rx: Receiver<Ev>,
    pub dir: PathBuf,
    pub session_path: PathBuf,
}

pub fn open(g: &Global) -> Result<Ctx> {
    session::ensure_dir(&g.data)?;
    let identity = Identity::load_or_create(&g.data.join("identity.key"))?;
    let history_key = session::load_or_create_key(&g.data.join("history.key"))?;
    let data_dir = g.data.join("core");
    session::ensure_dir(&data_dir)?;
    let session_path = g.data.join("session.json");
    let (tx, rx) = channel();
    let delegate = Arc::new(CliDelegate {
        tx,
        session_path: session_path.clone(),
    });
    let cfg = ServiceConfig {
        data_dir,
        api_base: g.api.clone(),
        relay_urls: g.relay.clone(),
        device_name: g.name.clone(),
        platform: Platform::Cli,
        device_secret: identity.seed(),
        history_key,
        force_relay: g.force_relay,
        allow_insecure_http: g.insecure_http,
        mdns: !g.force_relay,
    };
    let svc = FsyncService::new(cfg, delegate)?;
    if let Some(t) = session::load(&session_path)? {
        svc.set_session(t)?;
    }
    Ok(Ctx {
        svc,
        rx,
        dir: g.data.clone(),
        session_path,
    })
}

impl Ctx {
    /// Node up, account state known. `need_paired` rejects a device that is not in the list.
    pub fn online(&self, need_paired: bool) -> Result<AccountState> {
        self.svc.start()?;
        let state = self.svc.ensure_account()?;
        if need_paired && state == AccountState::NeedsPairing {
            bail!("this device is not paired yet: run `pair offer` here and `pair join` elsewhere");
        }
        Ok(state)
    }

    /// Blocks for the next event matching `pick` until `deadline`.
    pub fn wait_event<T>(
        &self,
        deadline: Instant,
        mut pick: impl FnMut(Ev) -> Option<T>,
    ) -> Result<T> {
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(ev) => {
                    report_side_events(&ev);
                    if let Some(v) = pick(ev) {
                        return Ok(v);
                    }
                }
                Err(RecvTimeoutError::Timeout) => bail!("timed out"),
                Err(RecvTimeoutError::Disconnected) => bail!("event channel closed"),
            }
        }
    }

    /// "lan" | "direct" | "relay" for the best live path to `node`, JSON null without one.
    pub fn path_tier(&self, node: &str) -> Option<&'static str> {
        self.svc.peer_path_tier(node).map(|t| t.as_str())
    }

    /// Hex id or name -> a paired device other than this one.
    pub fn resolve(&self, node: &str) -> Result<Device> {
        let mut hits: Vec<Device> = self
            .svc
            .devices()
            .into_iter()
            .filter(|d| !d.is_self && (d.node_id == node || d.name == node))
            .collect();
        match hits.len() {
            1 => Ok(hits.remove(0)),
            0 => bail!("no paired device matches {node}"),
            _ => bail!("{node} matches several devices; use the node id"),
        }
    }

    /// Presence reports who is connected shortly after start. Returns as soon as any other
    /// device shows online (at once when there is none), or after `max`.
    pub fn settle_presence(&self, max: Duration) {
        let end = Instant::now() + max;
        loop {
            let others: Vec<_> = self
                .svc
                .devices()
                .into_iter()
                .filter(|d| !d.is_self)
                .collect();
            if others.is_empty() || others.iter().any(|d| d.online) || Instant::now() >= end {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Polls until the device is online (backend presence or live connection).
    pub fn wait_online(&self, node: &str, wait: Duration) -> Result<Device> {
        let end = Instant::now() + wait;
        loop {
            let d = self.resolve(node)?;
            if d.online || d.status != fsync_core::service::DeviceStatus::Offline {
                return Ok(d);
            }
            if Instant::now() >= end {
                bail!("{node} did not come online");
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }
}

/// One JSON object per line on stdout, flushed (scripts read these).
pub fn emit(v: serde_json::Value) {
    let mut out = std::io::stdout().lock();
    // A closed stdout (broken pipe) ends the command at its next write; nothing else to do.
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

/// Account changes and background errors are never swallowed: stdout event / stderr line.
fn report_side_events(ev: &Ev) {
    match ev {
        Ev::Account(a) => emit(crate::events::account_json(a)),
        Ev::Error(e) => eprintln!("warning: {e}"),
        _ => {}
    }
}
