//! Presence WebSocket client: challenge/auth, hello (once the relay is ready, again on every
//! address change), 20 s heartbeat + ping with a liveness check, reconnect with backoff.

use super::{client::ApiClient, presence_session::session, ws_util::jitter};
use crate::{error::CoreError, identity::Identity, net::AddrFeed};
use fsync_proto::presence::ServerMsg;
use std::{sync::Arc, time::Duration};
use tokio::{net::TcpStream, sync::mpsc, task::JoinHandle, time};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

pub const HEARTBEAT: Duration = Duration::from_secs(20);
pub(super) const HANDSHAKE_WAIT: Duration = Duration::from_secs(10);
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(60);
/// Server closes with this code on token expiry / revoked session.
pub(super) const CLOSE_UNAUTHORIZED: u16 = 4401;

#[derive(Debug, Clone)]
pub enum PresenceEvent {
    Connected,
    Disconnected,
    Server(ServerMsg),
    /// The refresh token is no longer valid: the app must log in again. The task has stopped.
    AuthExpired,
}

/// Stops the background task when dropped.
pub struct PresenceHandle(JoinHandle<()>);

impl Drop for PresenceHandle {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(super) type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub(super) enum End {
    /// Closed because the access token is no longer accepted.
    Unauthorized,
    Lost,
}

pub fn spawn_presence(
    api: Arc<ApiClient>,
    me: Identity,
    feed: AddrFeed,
    out: mpsc::Sender<PresenceEvent>,
) -> PresenceHandle {
    PresenceHandle(tokio::spawn(async move {
        let mut backoff = BACKOFF_MIN;
        loop {
            let mut connected = false;
            let end = session(&api, &me, &feed, &out, &mut connected).await;
            if connected {
                backoff = BACKOFF_MIN;
                let _ = out.send(PresenceEvent::Disconnected).await;
            }
            match end {
                Ok(End::Unauthorized) => match api.refresh_now().await {
                    Ok(()) => continue,
                    Err(CoreError::AuthExpired) => {
                        let _ = out.send(PresenceEvent::AuthExpired).await;
                        return;
                    }
                    Err(e) => tracing::debug!("presence token refresh failed: {e}"),
                },
                Ok(End::Lost) => {}
                Err(CoreError::AuthExpired) => {
                    let _ = out.send(PresenceEvent::AuthExpired).await;
                    return;
                }
                Err(e) => tracing::debug!("presence session failed: {e}"),
            }
            if out.is_closed() {
                return;
            }
            time::sleep(jitter(backoff)).await;
            backoff = (backoff * 2).min(BACKOFF_MAX);
        }
    }))
}
