//! Existing-device side of pair/1.

use super::{
    ListBackend, PairSecret, STEP_TIMEOUT, USER_TIMEOUT,
    initiator_steps::{listed_head, own_entry, publish_new_list, recv_step, zeroizing_vec},
    kdf::{PairKeys, Role, transcript},
    spake, wire,
};
use crate::{
    error::{CoreError, PairError, Result},
    events::{EventSink, PairDecision, PairingEvent},
    identity::Identity,
    net::convert::node_id_of,
    trust::TrustManager,
};
use fsync_proto::{
    PairFrame,
    names::validate_device_name,
    pairing::{AbortReason, PAIR_VERSION},
};
use iroh::endpoint::{Connection, RecvStream, SendStream};

/// Run the handshake on `conn` (already dialed to the new device). Returns the published seq.
pub async fn run_initiator<B: ListBackend>(
    conn: &Connection,
    me: &Identity,
    secret: &PairSecret,
    trust: &TrustManager,
    events: &EventSink,
    backend: &B,
) -> Result<u64> {
    let (mut tx, mut rx) = conn.open_bi().await.map_err(CoreError::net)?;
    let res = session(conn, me, secret, trust, events, backend, &mut tx, &mut rx).await;
    match &res {
        Ok(seq) => {
            let peer = node_id_of(conn);
            events.pairing(PairingEvent::Completed { peer, seq: *seq });
            let _ = tx.finish();
            conn.close(0u32.into(), b"done");
        }
        Err(e) => {
            let reason = match e {
                CoreError::Pair(PairError::BadMac) => AbortReason::BadMac,
                CoreError::Pair(PairError::Rejected) => AbortReason::Rejected,
                _ => AbortReason::Internal,
            };
            let _ = wire::send(&mut tx, &PairFrame::Abort { reason }).await;
            let _ = tx.finish();
            events.pairing(PairingEvent::Failed {
                reason: e.to_string(),
            });
            conn.close(1u32.into(), b"abort");
        }
    }
    res
}

#[allow(clippy::too_many_arguments)]
async fn session<B: ListBackend>(
    conn: &Connection,
    me: &Identity,
    secret: &PairSecret,
    trust: &TrustManager,
    events: &EventSink,
    backend: &B,
    tx: &mut SendStream,
    rx: &mut RecvStream,
) -> Result<u64> {
    let (e_id, n_id) = (me.node_id(), node_id_of(conn));
    let mode = secret.mode();
    let nonce_e = wire::fresh_nonce()?;
    wire::send(
        tx,
        &PairFrame::Hello {
            v: PAIR_VERSION,
            mode,
            node_id: e_id,
            nonce: nonce_e,
        },
    )
    .await?;
    let PairFrame::HelloReply { nonce: nonce_n } = recv_step(rx).await? else {
        return Err(PairError::Protocol("expected hello reply").into());
    };

    let (shared, msg_e, msg_n) = match secret {
        PairSecret::Code(code) => {
            let (st, msg_e) = spake::start_existing(code, &e_id, &n_id);
            wire::send(tx, &PairFrame::Spake { msg: msg_e.clone() }).await?;
            let PairFrame::Spake { msg: msg_n } = recv_step(rx).await? else {
                return Err(PairError::Protocol("expected spake").into());
            };
            let key = st.finish(&msg_n)?;
            (zeroizing_vec(&key), msg_e, msg_n)
        }
        PairSecret::Qr(_) => {
            let key = secret
                .qr_key(&e_id, &n_id)
                .ok_or(CoreError::Invalid("qr secret"))?;
            (zeroizing_vec(key.as_ref()), vec![], vec![])
        }
    };
    let ts = transcript(mode, &e_id, &n_id, (&nonce_e, &nonce_n), &msg_e, &msg_n);
    let keys = PairKeys::derive(&shared, &ts);

    wire::send(
        tx,
        &PairFrame::Confirm {
            mac: keys.mac(Role::Existing, &ts),
        },
    )
    .await?;
    let PairFrame::Confirm { mac } = recv_step(rx).await? else {
        return Err(PairError::Protocol("expected confirm").into());
    };
    if !keys.verify(Role::New, &ts, &mac) {
        return Err(PairError::BadMac.into());
    }
    // Tell the new device who is adding it (shown in its "Pair with <name>?" prompt).
    let (my_name, my_platform) = own_entry(trust, &e_id)?;
    wire::send(
        tx,
        &PairFrame::DeviceInfo {
            name: my_name,
            platform: my_platform,
        },
    )
    .await?;
    let PairFrame::DeviceInfo { name, platform } = recv_step(rx).await? else {
        return Err(PairError::Protocol("expected device info").into());
    };
    validate_device_name(&name)?;

    let (decision, answer) = PairDecision::channel();
    events.pairing(PairingEvent::ConfirmRequest {
        peer: n_id,
        name: name.clone(),
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
    // The new device must also consent locally before anything is published.
    match wire::recv(rx, USER_TIMEOUT + STEP_TIMEOUT).await? {
        PairFrame::Accept => {}
        PairFrame::Abort { reason } => return Err(PairError::PeerAborted(reason).into()),
        _ => return Err(PairError::Protocol("expected accept").into()),
    }

    // A device that lost its trust (history reset) is still in the list under the same node id:
    // nothing to publish, hand it the current head. It was SAS-confirmed and consented above.
    let signed = match listed_head(trust, &n_id) {
        Some(head) => head,
        None => {
            let signed = publish_new_list(me, trust, backend, n_id, name, platform).await?;
            trust.ingest(&signed)?;
            signed
        }
    };
    let seq = signed.decode()?.seq;
    // Head only: N trusts it because E signed it on this MAC-confirmed, SAS-checked channel.
    wire::send(
        tx,
        &PairFrame::SignedList {
            chain: vec![signed],
        },
    )
    .await?;
    match recv_step(rx).await? {
        PairFrame::Done => Ok(seq),
        PairFrame::Abort { reason } => Err(PairError::PeerAborted(reason).into()),
        _ => Err(PairError::Protocol("expected done").into()),
    }
}
