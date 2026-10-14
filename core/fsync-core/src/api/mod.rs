//! Backend client: HTTP (device list, pairing, auth) and the presence WebSocket.

mod check;
mod client;
mod hello_pacer;
mod limits;
mod lists;
mod presence;
mod presence_session;
mod tls;
mod ws_util;

pub use client::{ApiClient, TokenCallback};
pub use presence::{PresenceEvent, PresenceHandle, spawn_presence};
