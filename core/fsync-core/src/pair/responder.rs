//! New-device side of pair/1: runs inside the pair/1 protocol handler.
//!
//! The new device never pins anything on the strength of the other side alone: after the key
//! confirmation it shows "Pair with <E name>?" plus the SAS and blocks on an explicit local
//! decision before it will accept (and pin) the signed list.

use super::{
    PairSecret, STEP_TIMEOUT, USER_TIMEOUT,
    kdf::{PairKeys, Role, transcript},
    spake,
    ticket::PairTicket,
    wire,
};
use crate::{
    error::{CoreError, PairError, Result},
    events::{EventSink, PairDecision, PairingEvent},
    identity::Identity,
    net::convert::node_id_of,
    trust::TrustManager,
};
use fsync_proto::{
    NodeId, PairFrame,
    names::validate_device_name,
    pairing::{AbortReason, PAIR_VERSION},
};
use iroh::endpoint::{Connection, RecvStream, SendStream};
use std::time::Duration;

/// First bi stream must open quickly: a silent connector must not hold a pairing slot.
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(10);
/// Bound on waiting for the existing device to close after the last frame.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

/// Emits `Completed` once the list is pinned, even if the handler future is dropped during the
/// drain (router shutdown): the device is paired then, whatever happens to the connection.
struct CompletedOnDrop<'a> {
    events: &'a EventSink,
    peer: NodeId,
    pinned: Option<u64>,
}

impl Drop for CompletedOnDrop<'_> {
    fn drop(&mut self) {
        if let Some(seq) = self.pinned.take() {
            self.events.pairing(PairingEvent::Completed {
                peer: self.peer,
                seq,
            });
        }
    }
}

/// Serve one session; reports the outcome through the window ticket.
///
/// `Completed` comes after the drain: after the existing device closed the connection (it does
/// so once it has read `Done`), or after `DRAIN_TIMEOUT` if it never closes. A listener may then
/// shut the node down (the CLI exits) without cutting `Done` off in the normal case.
pub async fn run_responder(
    conn: Connection,
    mut ticket: PairTicket,
    me: &Identity,
    trust: &TrustManager,
    events: &EventSink,
) -> Result<()> {
    let mut completed = CompletedOnDrop {
        events,
        peer: node_id_of(&conn),
        pinned: None,
    };
    let res = async {
        let (mut tx, mut rx) = tokio::time::timeout(ACCEPT_TIMEOUT, conn.accept_bi())
            .await
            .map_err(|_| CoreError::Pair(PairError::Timeout))?
            .map_err(CoreError::net_chain)?;
        let r = session(&conn, &mut ticket, me, trust, events, &mut tx, &mut rx).await;
        if let Ok(seq) = &r {
            completed.pinned = Some(*seq);
        }
        if let Err(e) = &r {
            let reason = match e {
                CoreError::Pair(PairError::BadMac) => AbortReason::BadMac,
                CoreError::Pair(PairError::Rejected | PairError::Timeout) => AbortReason::Rejected,
                CoreError::Pair(PairError::PeerAborted(_)) => AbortReason::Rejected,
                _ => AbortReason::Internal,
            };
            let _ = wire::send(&mut tx, &PairFrame::Abort { reason }).await;
        }
        let _ = tx.finish();
        // Let the final frame drain before the connection goes away (bounded).
        let drained = tokio::time::timeout(DRAIN_TIMEOUT, conn.closed()).await;
        tracing::debug!(closed = ?drained.as_ref().ok(), ok = r.is_ok(), "pair/1 responder drained");
        r
    }
    .await;
    if let Err(e) = &res {
        events.pairing(PairingEvent::Failed {
            reason: e.to_string(),
        });
    }
    let reply = match &res {
        Ok(s) => Ok(*s),
        Err(CoreError::Pair(p)) => Err(CoreError::Pair(p.clone())),
        Err(e) => Err(CoreError::Net(e.to_string())),
    };
    ticket.finish(reply);
    drop(completed); // emits `Completed` when pinned
    res.map(|_| ())
}

async fn session(
    conn: &Connection,
    ticket: &mut PairTicket,
    me: &Identity,
    trust: &TrustManager,
    events: &EventSink,
    tx: &mut SendStream,
    rx: &mut RecvStream,
) -> Result<u64> {
    let (secret, info) = (ticket.secret.clone(), ticket.info.clone());
    let (n_id, e_id) = (me.node_id(), node_id_of(conn));
    let PairFrame::Hello {
        v,
        mode,
        node_id,
        nonce: nonce_e,
    } = wire::recv(rx, STEP_TIMEOUT).await?
    else {
        return Err(PairError::Protocol("expected hello").into());
    };
    if v != PAIR_VERSION {
        return Err(PairError::Protocol("unsupported version").into());
    }
    // No downgrade between modes, and the claimed id must be the authenticated peer.
    if mode != secret.mode() || node_id != e_id {
        return Err(PairError::Protocol("hello mismatch").into());
    }
    let nonce_n = wire::fresh_nonce()?;
    wire::send(tx, &PairFrame::HelloReply { nonce: nonce_n }).await?;

    let (shared, msg_e, msg_n) = match &secret {
        PairSecret::Code(code) => {
            let PairFrame::Spake { msg: msg_e } = wire::recv(rx, STEP_TIMEOUT).await? else {
                return Err(PairError::Protocol("expected spake").into());
            };
            let (st, msg_n) = spake::start_new(code, &e_id, &n_id);
            wire::send(tx, &PairFrame::Spake { msg: msg_n.clone() }).await?;
            let key = st.finish(&msg_e)?;
            (zeroize::Zeroizing::new(key.to_vec()), msg_e, msg_n)
        }
        PairSecret::Qr(_) => {
            let key = secret
                .qr_key(&e_id, &n_id)
                .ok_or(CoreError::Invalid("qr secret"))?;
            (zeroize::Zeroizing::new(key.to_vec()), vec![], vec![])
        }
    };
    let ts = transcript(mode, &e_id, &n_id, (&nonce_e, &nonce_n), &msg_e, &msg_n);
    let keys = PairKeys::derive(&shared, &ts);

    let PairFrame::Confirm { mac } = wire::recv(rx, STEP_TIMEOUT).await? else {
        return Err(PairError::Protocol("expected confirm").into());
    };
    // Abort on the first bad MAC; failures count against the window's guess limit.
    if !keys.verify(Role::Existing, &ts, &mac) {
        return Err(PairError::BadMac.into());
    }
    // Only a peer that knows the secret may take over the window (stops its TTL).
    ticket.claim()?;
    wire::send(
        tx,
        &PairFrame::Confirm {
            mac: keys.mac(Role::New, &ts),
        },
    )
    .await?;
    wire::send(
        tx,
        &PairFrame::DeviceInfo {
            name: info.name.clone(),
            platform: info.platform,
        },
    )
    .await?;
    let PairFrame::DeviceInfo { name, platform } = wire::recv(rx, STEP_TIMEOUT).await? else {
        return Err(PairError::Protocol("expected device info").into());
    };
    validate_device_name(&name)?;

    // Explicit local consent, same SAS words as on the existing device. No answer = no pairing.
    let (decision, answer) = PairDecision::channel();
    events.pairing(PairingEvent::ConfirmRequest {
        peer: e_id,
        name,
        platform,
        mode,
        sas: keys.sas().to_string(),
        decision,
    });
    match tokio::time::timeout(USER_TIMEOUT, answer).await {
        Ok(Ok(true)) => {}
        Ok(_) => return Err(PairError::Rejected.into()),
        Err(_) => return Err(PairError::Timeout.into()),
    }
    wire::send(tx, &PairFrame::Accept).await?;

    // The existing device waits for its user too; allow for that.
    let PairFrame::SignedList { chain } = wire::recv(rx, USER_TIMEOUT + STEP_TIMEOUT).await? else {
        return Err(PairError::Protocol("expected signed list").into());
    };
    // Normal case: only the head, signed by E over this authenticated, user-approved channel.
    // A full chain from genesis is still accepted (and verified end to end).
    let seq = match chain.as_slice() {
        [head] => trust.pin_paired_head(head, &n_id, &e_id, info.account_id)?,
        _ => trust.pin_paired_chain(&chain, &n_id, &e_id)?,
    };
    // Paired from here on: a dropped session still reports this seq, not `Closed`.
    ticket.set_pinned(seq);
    wire::send(tx, &PairFrame::Done).await?;
    // `Completed` is emitted by `run_responder` after the drain.
    Ok(seq)
}
