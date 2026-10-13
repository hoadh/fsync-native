//! Size and scheme limits for the backend connection.

use crate::error::{CoreError, Result};
use reqwest::Response;
use serde::de::DeserializeOwned;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

/// Largest HTTP response body we buffer.
pub const MAX_RESPONSE_BYTES: usize = 1 << 20;
/// Largest presence WebSocket message/frame we accept.
pub const MAX_WS_BYTES: usize = 64 * 1024;

/// `base` must be https unless the caller explicitly allows plain http (dev/test backends).
pub fn check_scheme(base: &str, allow_insecure_http: bool) -> Result<()> {
    let scheme = base.split_once("://").map(|(s, _)| s.to_ascii_lowercase());
    match scheme.as_deref() {
        Some("https") => Ok(()),
        Some("http") if allow_insecure_http => Ok(()),
        _ => Err(CoreError::Invalid("backend url must be https")),
    }
}

pub fn ws_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(MAX_WS_BYTES))
        .max_frame_size(Some(MAX_WS_BYTES))
}

/// Read the body up to `MAX_RESPONSE_BYTES` and parse it as JSON.
pub async fn capped_json<T: DeserializeOwned>(mut r: Response) -> Result<T> {
    if r.content_length()
        .is_some_and(|n| n > MAX_RESPONSE_BYTES as u64)
    {
        return Err(CoreError::Http("response too large".into()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = r
        .chunk()
        .await
        .map_err(|e| CoreError::Http(e.without_url().to_string()))?
    {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(CoreError::Http("response too large".into()));
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|e| CoreError::Http(format!("bad response body: {e}")))
}
