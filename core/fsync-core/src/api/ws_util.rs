//! Presence socket helpers.

use super::presence::{CLOSE_UNAUTHORIZED, End, Ws};
use crate::error::{CoreError, Result};
use fsync_proto::presence::{ClientMsg, ServerMsg};
use futures_util::{SinkExt, StreamExt};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use tokio_tungstenite::tungstenite::{Message, protocol::frame::coding::CloseCode};

/// Time of the last inbound frame (shared between the read future and the liveness check).
pub(super) struct Seen {
    base: Instant,
    ms: AtomicU64,
}

impl Seen {
    pub(super) fn new() -> Self {
        Self {
            base: Instant::now(),
            ms: AtomicU64::new(0),
        }
    }
    fn stamp(&self) {
        let ms = u64::try_from(self.base.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.ms.store(ms, Ordering::Relaxed);
    }
    pub(super) fn idle(&self) -> Duration {
        self.base
            .elapsed()
            .saturating_sub(Duration::from_millis(self.ms.load(Ordering::Relaxed)))
    }
}

pub(super) fn jitter(d: Duration) -> Duration {
    let mut b = [0u8; 1];
    let _ = getrandom::fill(&mut b);
    d + d.mul_f64(f64::from(b[0]) / 1024.0)
}

pub(super) fn ws_url(base: &str) -> String {
    let swapped = match base.split_once("://") {
        Some(("https", rest)) => format!("wss://{rest}"),
        Some(("http", rest)) => format!("ws://{rest}"),
        _ => base.to_owned(),
    };
    format!("{swapped}/v1/presence")
}

pub(super) async fn send(ws: &mut Ws, m: &ClientMsg) -> Result<()> {
    let text = serde_json::to_string(m).map_err(|e| CoreError::Ws(e.to_string()))?;
    ws.send(Message::text(text))
        .await
        .map_err(|e| CoreError::Ws(e.to_string()))
}

pub(super) async fn ping(ws: &mut Ws) -> Result<()> {
    ws.send(Message::Ping(Default::default()))
        .await
        .map_err(|e| CoreError::Ws(e.to_string()))
}

/// Next server message; `Err(End)` when the socket closed. `seen` is stamped on every inbound
/// frame (pongs included) so the caller can tell a silent, half-dead socket from a quiet one.
pub(super) async fn next(ws: &mut Ws, seen: &Seen) -> std::result::Result<ServerMsg, End> {
    loop {
        let frame = ws.next().await;
        seen.stamp();
        match frame {
            Some(Ok(Message::Text(t))) => match serde_json::from_str(t.as_str()) {
                Ok(m) => return Ok(m),
                Err(_) => return Err(End::Lost),
            },
            Some(Ok(Message::Close(Some(f)))) if f.code == CloseCode::from(CLOSE_UNAUTHORIZED) => {
                return Err(End::Unauthorized);
            }
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return Err(End::Lost),
            Some(Ok(_)) => {}
        }
    }
}
