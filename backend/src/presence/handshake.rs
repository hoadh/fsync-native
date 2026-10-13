//! Presence socket helpers and the authentication handshake.

use crate::{
    auth::extractor::{AuthSession, authenticate},
    devices::repo as devices,
    error::ApiResult,
    pairing::repo as pairing,
    state::AppState,
    util::random_bytes,
};
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use fsync_proto::{
    NodeId, Nonce,
    presence::{ClientMsg, ServerMsg, verify_challenge},
};
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

const AUTH_TIMEOUT: Duration = Duration::from_secs(10);

/// An authenticated presence socket.
pub struct Authed {
    pub account: Uuid,
    pub node: NodeId,
    /// Node is not in the device list yet (live pairing offer only).
    pub pending: bool,
    pub session: AuthSession,
}

pub async fn send(sock: &mut WebSocket, msg: &ServerMsg) -> bool {
    match serde_json::to_string(msg) {
        Ok(t) => sock.send(Message::text(t)).await.is_ok(),
        Err(_) => false,
    }
}

pub async fn close(mut sock: WebSocket, code: u16, reason: &'static str) {
    let frame = CloseFrame {
        code,
        reason: reason.into(),
    };
    let _ = sock.send(Message::Close(Some(frame))).await;
}

pub fn parse(msg: &Message) -> Option<ClientMsg> {
    match msg {
        Message::Text(t) => serde_json::from_str(t.as_str()).ok(),
        _ => None,
    }
}

/// Listed node ids; one retry on a database error. Callers fail closed on a second error.
pub async fn listed_nodes(db: &PgPool, account: Uuid) -> ApiResult<Vec<NodeId>> {
    match devices::node_ids(db, account).await {
        Ok(v) => Ok(v),
        Err(_) => {
            tokio::time::sleep(Duration::from_millis(100)).await;
            devices::node_ids(db, account).await
        }
    }
}

/// Where an unlisted (pending) node stands.
pub enum PairingState {
    Pending,
    Listed,
    Expired,
}

/// Re-check a pending socket: it may stay only while its pairing offer is live or until listed.
pub async fn pairing_state(db: &PgPool, account: Uuid, node: &NodeId) -> ApiResult<PairingState> {
    if listed_nodes(db, account).await?.contains(node) {
        return Ok(PairingState::Listed);
    }
    Ok(if pairing::has_live_offer(db, account, node).await? {
        PairingState::Pending
    } else {
        PairingState::Expired
    })
}

/// Challenge -> Auth (access token + device-key signature). `Ok(None)` = reject.
pub async fn authenticate_socket(s: &AppState, sock: &mut WebSocket) -> ApiResult<Option<Authed>> {
    let nonce = Nonce::from(random_bytes::<32>()?);
    if !send(sock, &ServerMsg::Challenge { nonce }).await {
        return Ok(None);
    }
    let first = tokio::time::timeout(AUTH_TIMEOUT, sock.recv()).await;
    let Ok(Some(Ok(msg))) = first else {
        return Ok(None);
    };
    let Some(ClientMsg::Auth {
        access,
        node_id,
        sig,
    }) = parse(&msg)
    else {
        return Ok(None);
    };
    let Ok(session) = authenticate(s, &access).await else {
        return Ok(None);
    };
    if verify_challenge(&node_id, &nonce, &sig).is_err() {
        return Ok(None);
    }
    let account = session.account;
    let listed = listed_nodes(&s.db, account).await?.contains(&node_id);
    // An unlisted node may connect only while it has a live pairing offer.
    if !listed && !pairing::has_live_offer(&s.db, account, &node_id).await? {
        return Ok(None);
    }
    Ok(Some(Authed {
        account,
        node: node_id,
        pending: !listed,
        session,
    }))
}
