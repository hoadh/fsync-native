//! Presence WebSocket test client.

use super::{Session, TestApp, devices::Device};
use fsync_proto::{
    Nonce,
    presence::{ClientMsg, ServerMsg, sign_challenge},
};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

pub type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub async fn send(ws: &mut Ws, msg: &ClientMsg) {
    let _ = ws
        .send(Message::text(serde_json::to_string(msg).unwrap()))
        .await;
}

/// Next server message, or `None` if the socket closed / timed out.
pub async fn recv(ws: &mut Ws) -> Option<ServerMsg> {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), ws.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => return serde_json::from_str(t.as_str()).ok(),
            Ok(Some(Ok(Message::Close(_)))) | Ok(Some(Err(_))) | Ok(None) | Err(_) => return None,
            Ok(Some(Ok(_))) => continue,
        }
    }
}

impl TestApp {
    /// Connect and read the challenge; returns the socket and nonce.
    pub async fn ws_connect(&self) -> (Ws, Nonce) {
        let (mut ws, _) = connect_async(format!("ws://{}/v1/presence", self.addr))
            .await
            .unwrap();
        let Some(ServerMsg::Challenge { nonce }) = recv(&mut ws).await else {
            panic!("expected challenge");
        };
        (ws, nonce)
    }

    /// Connect and authenticate as `dev`; the caller inspects the next message.
    pub async fn ws_auth(&self, s: &Session, dev: &Device) -> Ws {
        let (mut ws, nonce) = self.ws_connect().await;
        let auth = ClientMsg::Auth {
            access: s.access.clone(),
            node_id: dev.node,
            sig: sign_challenge(&dev.key, &nonce),
        };
        send(&mut ws, &auth).await;
        ws
    }
}
