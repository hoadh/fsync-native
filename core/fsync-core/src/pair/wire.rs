//! Length-prefixed postcard frames over a QUIC bi stream, with per-read deadlines.

use crate::error::{CoreError, PairError, Result};
use fsync_proto::{
    PairFrame,
    frame::{HEADER_LEN, decode_payload, encode_frame, payload_len},
};
use iroh::endpoint::{RecvStream, SendStream};
use std::time::Duration;

pub async fn send(tx: &mut SendStream, frame: &PairFrame) -> Result<()> {
    let bytes = encode_frame(frame)?;
    tx.write_all(&bytes).await.map_err(CoreError::net)
}

/// Read one frame; `PairError::Timeout` if the peer is silent for `wait`.
pub async fn recv(rx: &mut RecvStream, wait: Duration) -> Result<PairFrame> {
    tokio::time::timeout(wait, async {
        let mut hdr = [0u8; HEADER_LEN];
        rx.read_exact(&mut hdr).await.map_err(CoreError::net)?;
        let mut buf = vec![0u8; payload_len(hdr)?];
        rx.read_exact(&mut buf).await.map_err(CoreError::net)?;
        Ok(decode_payload::<PairFrame>(&buf)?)
    })
    .await
    .map_err(|_| CoreError::Pair(PairError::Timeout))?
}

/// 16 random bytes for the session transcript.
pub fn fresh_nonce() -> Result<[u8; 16]> {
    let mut n = [0u8; 16];
    getrandom::fill(&mut n).map_err(|_| CoreError::Invalid("rng"))?;
    Ok(n)
}
