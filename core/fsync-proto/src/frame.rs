//! u32-LE length-prefixed postcard frames over byte slices (pure; async I/O lives in core).

use crate::error::ProtoError;
use serde::{Serialize, de::DeserializeOwned};

/// 1 MiB payload + 256 KiB headroom for inline reps and framing overhead.
pub const MAX_FRAME_LEN: usize = 1024 * 1024 + 256 * 1024;
pub const HEADER_LEN: usize = 4;

pub fn encode_frame<T: Serialize>(msg: &T) -> Result<Vec<u8>, ProtoError> {
    let payload = postcard::to_stdvec(msg)?;
    if payload.len() > MAX_FRAME_LEN {
        return Err(ProtoError::FrameTooLarge {
            len: payload.len(),
            max: MAX_FRAME_LEN,
        });
    }
    let len = u32::try_from(payload.len()).map_err(|_| ProtoError::FrameTooLarge {
        len: payload.len(),
        max: MAX_FRAME_LEN,
    })?;
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Validate a received header and return the payload length. Call before allocating.
pub fn payload_len(header: [u8; HEADER_LEN]) -> Result<usize, ProtoError> {
    let len = u32::from_le_bytes(header) as usize;
    if len > MAX_FRAME_LEN {
        return Err(ProtoError::FrameTooLarge {
            len,
            max: MAX_FRAME_LEN,
        });
    }
    Ok(len)
}

/// Decode exactly one postcard payload (no header, no trailing bytes).
pub fn decode_payload<T: DeserializeOwned>(payload: &[u8]) -> Result<T, ProtoError> {
    let (v, rest) = postcard::take_from_bytes::<T>(payload)?;
    if !rest.is_empty() {
        return Err(ProtoError::TrailingBytes);
    }
    Ok(v)
}

/// Decode the first frame in `buf`; returns the value and total bytes consumed.
/// `Truncated{needed}` means more bytes are required.
pub fn decode_frame<T: DeserializeOwned>(buf: &[u8]) -> Result<(T, usize), ProtoError> {
    let Some(header) = buf.first_chunk::<HEADER_LEN>() else {
        return Err(ProtoError::Truncated {
            needed: HEADER_LEN - buf.len(),
        });
    };
    let len = payload_len(*header)?;
    let total = HEADER_LEN + len;
    if buf.len() < total {
        return Err(ProtoError::Truncated {
            needed: total - buf.len(),
        });
    }
    Ok((decode_payload(&buf[HEADER_LEN..total])?, total))
}
