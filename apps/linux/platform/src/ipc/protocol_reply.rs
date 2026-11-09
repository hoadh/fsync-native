//! Reply line and the bounded line reader shared by server and client.

use super::protocol::{MAX_LINE_BYTES, ProtocolError, invalid};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Read};

/// `{"ok":true}` or `{"ok":false,"error":"..."}`; `status` may carry extra JSON text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Free-form payload (e2e `status`, pair codes); absent for plain acks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Reply {
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
            data: None,
        }
    }
    pub fn ok_with(data: serde_json::Value) -> Self {
        Self {
            ok: true,
            error: None,
            data: Some(data),
        }
    }
    pub fn err(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
            data: None,
        }
    }
    pub fn to_line(&self) -> String {
        // A plain struct of bool/string/Value always serializes.
        serde_json::to_string(self).unwrap_or_else(|_| r#"{"ok":false,"error":"internal"}"#.into())
    }
}

/// Read one `\n`-terminated line of at most `MAX_LINE_BYTES`; never buffers more than that.
pub fn read_line_bounded<R: BufRead>(reader: &mut R) -> Result<String, ProtocolError> {
    let mut buf = Vec::new();
    let n = reader
        .by_ref()
        .take(MAX_LINE_BYTES as u64 + 1)
        .read_until(b'\n', &mut buf)
        .map_err(|e| ProtocolError::Io(e.to_string()))?;
    if buf.last() != Some(&b'\n') {
        return Err(if n > MAX_LINE_BYTES {
            ProtocolError::TooLong
        } else {
            ProtocolError::Truncated
        });
    }
    if n > MAX_LINE_BYTES {
        return Err(ProtocolError::TooLong);
    }
    String::from_utf8(buf).map_err(|_| invalid("not utf-8"))
}
