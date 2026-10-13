//! pair offer / pair join. Both sides must confirm the same three words.

use crate::{
    cmd_account::prompt_line,
    ctx::{Ctx, emit},
    events::Ev,
};
use anyhow::{Result, bail};
use fsync_core::service::{PairingEvent, PairingMode, PairingPeer};
use serde_json::json;
use std::time::{Duration, Instant};

fn confirm(c: &Ctx, peer: &PairingPeer, yes: bool) -> Result<()> {
    emit(json!({"event": "sas", "words": peer.sas_words, "peer": peer.node_id, "name": peer.name}));
    let ok = yes
        || prompt_line(&format!(
            "words [{}] match the other screen? [y/N] ",
            peer.sas_words
        ))?
        .eq_ignore_ascii_case("y");
    c.svc.pairing_confirm(peer.request_id, ok)?;
    if !ok {
        bail!("pairing rejected");
    }
    Ok(())
}

fn finish(c: &Ctx, deadline: Instant) -> Result<()> {
    c.wait_event(deadline, |ev| match ev {
        Ev::Pairing(PairingEvent::Completed { peer, seq }) => {
            emit(json!({"event": "paired", "peer": peer, "seq": seq}));
            Some(Ok(()))
        }
        Ev::Pairing(PairingEvent::Failed { reason }) => {
            Some(Err(anyhow::anyhow!("pairing failed: {reason}")))
        }
        _ => None,
    })?
}

pub fn offer(c: &Ctx, qr: bool, yes: bool, timeout: u64) -> Result<()> {
    c.online(false)?;
    let r = offer_inner(c, qr, yes, timeout);
    if r.is_err() {
        // Close the window instead of leaving it to its TTL.
        let _ = c.svc.pairing_cancel();
    }
    r
}

fn offer_inner(c: &Ctx, qr: bool, yes: bool, timeout: u64) -> Result<()> {
    let mode = if qr {
        PairingMode::Qr
    } else {
        PairingMode::Code
    };
    let o = c.svc.pairing_offer(mode)?;
    emit(json!({"event": "offer", "code": o.code, "qr": o.qr_payload, "expires_ms": o.expires_ms}));
    let deadline = Instant::now() + Duration::from_secs(timeout);
    let peer = c.wait_event(deadline, |ev| match ev {
        Ev::Pairing(PairingEvent::ConfirmRequest(p)) => Some(Ok(p)),
        Ev::Pairing(PairingEvent::Failed { reason }) => {
            Some(Err(anyhow::anyhow!("pairing failed: {reason}")))
        }
        _ => None,
    })??;
    confirm(c, &peer, yes)?;
    finish(c, deadline)
}

pub fn join(c: &Ctx, input: &str, yes: bool) -> Result<()> {
    c.online(true)?;
    let peer = c.svc.pairing_join(input)?;
    let r =
        confirm(c, &peer, yes).and_then(|()| finish(c, Instant::now() + Duration::from_secs(120)));
    if r.is_err() {
        let _ = c.svc.pairing_cancel();
    }
    r
}
