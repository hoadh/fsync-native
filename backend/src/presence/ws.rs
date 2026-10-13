//! GET /v1/presence (WebSocket). Flow: server `Challenge` -> client `Auth` (access token + device
//! key signature over the nonce) -> server `Peers` (implicit ack) -> `Hello`/`Heartbeat`
//! from client, `PeerUpdate`/`DeviceListChanged` from server. Failed auth closes the socket.
//! The socket also ends when the access token expires, the session is revoked, or the
//! device is removed from the list; a pending (pairing) socket also ends when its offer expires.

use super::{
    addrs::{HEARTBEATS_PER_MIN, HELLOS_PER_MIN, MsgLimiter, sanitize},
    handshake::{
        Authed, PairingState, authenticate_socket, close, listed_nodes, pairing_state, parse, send,
    },
    hub::{CLOSE_UNAUTHORIZED, Outbound},
};
use crate::{client_ip::client_key, error::ApiError, state::AppState, util::now_ms};
use axum::{
    Router,
    extract::{
        ConnectInfo, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::HeaderMap,
    response::{IntoResponse, Response},
    routing::get,
};
use fsync_proto::presence::{ClientMsg, ServerMsg};
use std::{net::SocketAddr, time::Duration};
use tokio::{sync::mpsc, time::Instant};

/// Frame and message cap (a Hello is well under 2 KiB).
const MAX_MESSAGE: usize = 8 * 1024;
const CLOSE_PROTOCOL: u16 = 4400;
const CLOSE_RATE: u16 = 4429;

type Closing = (u16, &'static str);

pub fn router() -> Router<AppState> {
    Router::new().route("/presence", get(upgrade))
}

async fn upgrade(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let ip = client_key(&s.cfg, &headers, peer);
    let Some(permit) = s.unauth.try_acquire(&ip) else {
        return ApiError::Unavailable.into_response();
    };
    ws.max_message_size(MAX_MESSAGE)
        .max_frame_size(MAX_MESSAGE)
        .on_upgrade(move |mut sock| async move {
            let authed = authenticate_socket(&s, &mut sock).await;
            drop(permit); // handshake over: frees the unauthenticated-socket slot
            session(s, sock, authed).await
        })
}

async fn session(s: AppState, mut sock: WebSocket, authed: Result<Option<Authed>, ApiError>) {
    let a = match authed {
        Ok(Some(a)) => a,
        Ok(None) => return close(sock, CLOSE_UNAUTHORIZED, "unauthorized").await,
        Err(e) => {
            tracing::warn!(error = %e, "presence auth error");
            return close(sock, CLOSE_UNAUTHORIZED, "unauthorized").await;
        }
    };
    // Check membership before registering so a removed node never flashes online to its peers
    // (re-checked after registering in `run` to close the connect-vs-removal race).
    if !a.pending {
        match listed_nodes(&s.db, a.account).await {
            Ok(l) if l.contains(&a.node) => {}
            Ok(_) => return close(sock, CLOSE_UNAUTHORIZED, "removed").await,
            Err(_) => return close(sock, 1011, "internal").await,
        }
    }
    let (tx, mut rx) = mpsc::channel::<Outbound>(64);
    let conn = s
        .hub
        .connect(a.account, a.node, a.pending, a.session.sid, tx);
    let (code, reason) = run(&s, &mut sock, &a, conn, &mut rx).await;
    s.hub.disconnect(a.account, a.node, conn);
    close(sock, code, reason).await;
}

async fn run(
    s: &AppState,
    sock: &mut WebSocket,
    a: &Authed,
    conn: u64,
    rx: &mut mpsc::Receiver<Outbound>,
) -> Closing {
    // Re-check membership after registering with the hub (closes the connect-vs-removal race);
    // a database error fails closed.
    let peers = if a.pending {
        vec![]
    } else {
        match listed_nodes(&s.db, a.account).await {
            Ok(l) if l.contains(&a.node) => s.hub.snapshot(a.account, &a.node, &l),
            Ok(_) => return (CLOSE_UNAUTHORIZED, "removed"),
            Err(_) => return (1011, "internal"),
        }
    };
    if !send(sock, &ServerMsg::Peers { peers }).await {
        return (1001, "gone");
    }
    let token_left = u64::try_from(a.session.exp - now_ms() / 1000).unwrap_or(0);
    let token_deadline = Instant::now() + Duration::from_secs(token_left);
    let mut deadline = Instant::now() + s.cfg.heartbeat_timeout;
    let mut hellos = MsgLimiter::new(HELLOS_PER_MIN);
    let mut heartbeats = MsgLimiter::new(HEARTBEATS_PER_MIN);
    // A pending (pairing) socket lives only while its offer does, unless it gets listed.
    let mut pending = a.pending;
    let mut recheck = tokio::time::interval_at(
        Instant::now() + s.cfg.pending_recheck,
        s.cfg.pending_recheck,
    );
    loop {
        tokio::select! {
            out = rx.recv() => match out {
                None => return (1001, "superseded"),
                Some(Outbound::Close(code, reason)) => return (code, reason),
                Some(Outbound::Msg(msg)) => if !send(sock, &msg).await { return (1001, "gone") },
            },
            _ = recheck.tick(), if pending => match pairing_state(&s.db, a.account, &a.node).await {
                Ok(PairingState::Pending) => {}
                Ok(PairingState::Listed) => pending = false,
                Ok(PairingState::Expired) => return (CLOSE_UNAUTHORIZED, "pairing offer expired"),
                Err(_) => return (1011, "internal"),
            },
            _ = tokio::time::sleep_until(token_deadline) => return (CLOSE_UNAUTHORIZED, "token expired"),
            _ = tokio::time::sleep_until(deadline) => return (1001, "heartbeat timeout"),
            incoming = sock.recv() => {
                let Some(Ok(msg)) = incoming else { return (1001, "gone") };
                if matches!(msg, Message::Close(_)) { return (1000, "bye"); }
                if matches!(msg, Message::Ping(_) | Message::Pong(_)) { deadline = Instant::now() + s.cfg.heartbeat_timeout; continue; }
                match parse(&msg) {
                    Some(ClientMsg::Heartbeat) => {
                        if !heartbeats.allow() { return (CLOSE_RATE, "too many heartbeats"); }
                    }
                    Some(ClientMsg::Hello { addrs }) => {
                        if !hellos.allow() { return (CLOSE_RATE, "too many hello"); }
                        let Some(addrs) = sanitize(addrs, s.cfg.dev_mode) else { return (CLOSE_PROTOCOL, "protocol") };
                        s.hub.set_addrs(a.account, a.node, conn, addrs);
                    }
                    _ => return (CLOSE_PROTOCOL, "protocol"),
                }
                deadline = Instant::now() + s.cfg.heartbeat_timeout;
            }
        }
    }
}
