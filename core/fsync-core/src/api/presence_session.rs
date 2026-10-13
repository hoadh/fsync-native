//! One presence WebSocket session, from connect to loss.

use super::{
    hello_pacer::HelloPacer,
    limits::ws_config,
    presence::{End, HANDSHAKE_WAIT, HEARTBEAT, PresenceEvent},
    tls::client_config,
    ws_util::{Seen, next, ping, send, ws_url},
};
use crate::{
    api::ApiClient,
    error::{CoreError, Result},
    identity::Identity,
    net::AddrFeed,
};
use fsync_proto::presence::{ClientMsg, ServerMsg, sign_challenge};
use std::time::{Duration, Instant as StdInstant};
use tokio::{sync::mpsc, time};
use tokio_tungstenite::{Connector, connect_async_tls_with_config};

/// Short grace for the relay URL before the first `Hello`; it only saves one extra message
/// (a later relay URL is re-sent on change), so it must stay small. Returns at once when the
/// relay is already known, so reconnects with a healthy relay pay nothing.
const RELAY_READY_WAIT: Duration = Duration::from_millis(300);
/// Address changes arriving together are sent as one `Hello`.
const HELLO_DEBOUNCE: Duration = Duration::from_millis(200);
/// No inbound frame (pong included) for this long means the path is dead.
const LIVENESS: Duration = Duration::from_secs(HEARTBEAT.as_secs() * 2);

pub(super) async fn session(
    api: &ApiClient,
    me: &Identity,
    feed: &AddrFeed,
    out: &mpsc::Sender<PresenceEvent>,
    connected: &mut bool,
) -> Result<End> {
    let mut addrs = feed.rx.clone();
    if feed.expect_relay {
        // Bounded: with the relay not ready we still say hello (direct addresses only) and
        // follow up as soon as the relay URL shows up.
        let _ = time::timeout(RELAY_READY_WAIT, addrs.wait_for(|a| a.relay_url.is_some())).await;
    }
    let access = api.access_token()?;
    let connector = Connector::Rustls(client_config()?);
    let (mut ws, _) = connect_async_tls_with_config(
        ws_url(api.base()),
        Some(ws_config()),
        false,
        Some(connector),
    )
    .await
    .map_err(|e| CoreError::Ws(e.to_string()))?;

    let seen = Seen::new();
    let nonce = match time::timeout(HANDSHAKE_WAIT, next(&mut ws, &seen)).await {
        Ok(Ok(ServerMsg::Challenge { nonce })) => nonce,
        Ok(Err(end)) => return Ok(end),
        _ => return Ok(End::Lost),
    };
    let auth = ClientMsg::Auth {
        access,
        node_id: me.node_id(),
        sig: sign_challenge(me.signing_key(), &nonce),
    };
    send(&mut ws, &auth).await?;
    // The first `peers` message is the implicit ack; a close here means auth failed.
    let first = match time::timeout(HANDSHAKE_WAIT, next(&mut ws, &seen)).await {
        Ok(Ok(m @ ServerMsg::Peers { .. })) => m,
        Ok(Err(end)) => return Ok(end),
        _ => return Ok(End::Lost),
    };
    let mut sent = addrs.borrow_and_update().clone();
    send(
        &mut ws,
        &ClientMsg::Hello {
            addrs: sent.clone(),
        },
    )
    .await?;
    let mut pacer = HelloPacer::default();
    pacer.record(StdInstant::now());
    *connected = true;
    if out.send(PresenceEvent::Connected).await.is_err()
        || out.send(PresenceEvent::Server(first)).await.is_err()
    {
        return Ok(End::Lost);
    }

    let mut beat = time::interval(HEARTBEAT);
    beat.tick().await;
    let mut feed_open = true;
    let mut hello_at: Option<time::Instant> = None;
    loop {
        tokio::select! {
            m = next(&mut ws, &seen) => match m {
                Ok(msg) => {
                    // Never wait on a slow consumer: heartbeats must keep flowing. A full
                    // queue drops the session; the reconnect delivers a fresh peers snapshot.
                    if out.try_send(PresenceEvent::Server(msg)).is_err() {
                        tracing::debug!("presence consumer not keeping up; reconnecting");
                        return Ok(End::Lost);
                    }
                }
                Err(end) => return Ok(end),
            },
            _ = beat.tick() => {
                if seen.idle() > LIVENESS {
                    tracing::debug!("presence socket silent; reconnecting");
                    return Ok(End::Lost);
                }
                ping(&mut ws).await?;
                send(&mut ws, &ClientMsg::Heartbeat).await?;
            }
            changed = addrs.changed(), if feed_open => match changed {
                Ok(()) if hello_at.is_none() => {
                    let slot = pacer.next_slot(StdInstant::now());
                    let wait = slot.saturating_duration_since(StdInstant::now());
                    hello_at = Some(time::Instant::now() + wait.max(HELLO_DEBOUNCE));
                }
                Ok(()) => {}
                Err(_) => feed_open = false,
            },
            () = async { time::sleep_until(hello_at.unwrap_or_else(time::Instant::now)).await },
                if hello_at.is_some() =>
            {
                hello_at = None;
                let now = addrs.borrow_and_update().clone();
                if now != sent {
                    send(&mut ws, &ClientMsg::Hello { addrs: now.clone() }).await?;
                    pacer.record(StdInstant::now());
                    sent = now;
                }
            }
        }
    }
}
