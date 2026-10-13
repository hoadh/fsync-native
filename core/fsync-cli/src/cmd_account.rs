//! login, devices, history.

use crate::{
    args::Global,
    ctx::{self, Ctx, emit},
    session,
};
use anyhow::{Context, Result, bail};
use fsync_core::service::{AccountState, DeviceStatus, FsyncService};
use serde_json::json;
use std::io::{BufRead, Write};

pub fn prompt_line(msg: &str) -> Result<String> {
    eprint!("{msg}");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

fn state_name(s: AccountState) -> &'static str {
    match s {
        AccountState::Created => "created",
        AccountState::Ready => "ready",
        AccountState::NeedsPairing => "needs_pairing",
    }
}

pub fn login(g: &Global, email: &str, code: Option<String>, send_only: bool) -> Result<()> {
    let c = ctx::open(g)?;
    if code.is_none() {
        c.svc.login_start(email)?;
        if send_only {
            emit(json!({"event": "code_sent"}));
            return Ok(());
        }
    }
    let code = match code {
        Some(c) => c,
        None => prompt_line("code: ")?,
    };
    let tokens = c.svc.login_verify(email, &code)?;
    session::save(&c.session_path, &tokens).context("store session")?;
    let state = c.online(false)?;
    emit(json!({"event": "logged_in", "account": state_name(state), "node_id": c.svc.node_id()}));
    Ok(())
}

pub fn devices(c: &Ctx, as_json: bool) -> Result<()> {
    c.online(true)?;
    c.settle_presence(std::time::Duration::from_secs(2));
    let list: Vec<_> = c
        .svc
        .devices()
        .into_iter()
        .map(|d| {
            let status = match d.status {
                DeviceStatus::Offline => "offline",
                DeviceStatus::Lan => "lan",
                DeviceStatus::Relay => "relay",
            };
            json!({"node_id": d.node_id, "name": d.name, "platform": format!("{:?}", d.platform).to_lowercase(),
                "status": status, "online": d.online, "last_seen_ms": d.last_seen_ms, "self": d.is_self})
        })
        .collect();
    if as_json {
        emit(json!(list));
    } else {
        for d in &list {
            println!(
                "{} {} {} {}",
                d["node_id"], d["name"], d["status"], d["online"]
            );
        }
    }
    Ok(())
}

pub fn history(c: &Ctx, as_json: bool, query: Option<String>, limit: u32) -> Result<()> {
    let page = c.svc.history(query, None, limit)?;
    let items: Vec<_> = page
        .items
        .iter()
        .map(|i| {
            json!({"id": i.id, "outgoing": i.outgoing, "peer": i.peer, "kind": format!("{:?}", i.kind).to_lowercase(),
                "created_ms": i.created_ms, "total_size": i.total_size, "preview": i.preview, "pinned": i.pinned})
        })
        .collect();
    if as_json {
        emit(json!(items));
    } else {
        for i in &items {
            println!("{} {} {}", i["id"], i["kind"], i["preview"]);
        }
    }
    Ok(())
}

/// Moves the history aside so a service with a new key can start. Needs `--yes`.
pub fn history_reset(g: &Global, yes: bool) -> Result<()> {
    if !yes {
        bail!(
            "this abandons the history (kept as history.corrupt-*) and its pinned trust; pass --yes"
        );
    }
    FsyncService::reset_history(&g.data.join("core"))?;
    emit(json!({"event": "history_reset"}));
    Ok(())
}
