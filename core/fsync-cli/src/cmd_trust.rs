//! account reset-trust, trust resets / confirm / decline (ADR 012).

use crate::{
    cmd_account::state_name,
    ctx::{Ctx, emit},
};
use anyhow::{Result, bail};
use serde_json::json;

/// Replaces the account's device list with one that holds only this device.
pub fn reset(c: &Ctx, yes: bool) -> Result<()> {
    if !yes {
        bail!(
            "this makes this device the only member of the account's device list: every other \
             device must confirm the reset (and then pair again) or decline it. Pass --yes to \
             continue. A login from the last 5 minutes is required (run `login` first)."
        );
    }
    c.svc.start()?;
    let state = c.svc.reset_account_trust()?;
    emit(
        json!({"event": "trust_reset_done", "account": state_name(state), "node_id": c.svc.node_id()}),
    );
    Ok(())
}

/// Resets are found by syncing the device list, so go online first.
pub fn resets(c: &Ctx, as_json: bool) -> Result<()> {
    c.online(false)?;
    let list = c.svc.pending_trust_resets();
    if as_json {
        let items: Vec<_> = list
            .iter()
            .map(|r| json!({"reset_id": r.reset_id, "devices": r.devices, "device_names": r.device_names}))
            .collect();
        emit(json!(items));
    } else {
        for r in &list {
            println!(
                "{} devices={} names={:?}",
                r.reset_id, r.devices, r.device_names
            );
        }
    }
    Ok(())
}

pub fn confirm(c: &Ctx, id: &str) -> Result<()> {
    c.online(false)?; // the reset is only known after a sync
    let state = c.svc.confirm_trust_reset(id)?;
    emit(json!({"event": "trust_reset_confirmed", "reset_id": id, "account": state_name(state)}));
    Ok(())
}

pub fn decline(c: &Ctx, id: &str) -> Result<()> {
    c.online(false)?;
    c.svc.decline_trust_reset(id)?;
    emit(json!({"event": "trust_reset_declined", "reset_id": id}));
    Ok(())
}
