//! Typed length-prefixed postcard frames on QUIC streams (clip/1, file/1).

use crate::error::{CoreError, Result, TransferError};
use fsync_proto::frame::{HEADER_LEN, decode_payload, encode_frame, payload_len};
use iroh::endpoint::{RecvStream, SendStream};
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;

pub async fn send_frame<T: Serialize>(tx: &mut SendStream, msg: &T) -> Result<()> {
    let bytes = encode_frame(msg)?;
    tx.write_all(&bytes).await.map_err(CoreError::net)
}

/// Read one frame; `TransferError::Timeout` if the peer is silent for `wait`.
pub async fn recv_frame<T: DeserializeOwned>(rx: &mut RecvStream, wait: Duration) -> Result<T> {
    tokio::time::timeout(wait, async {
        let mut hdr = [0u8; HEADER_LEN];
        rx.read_exact(&mut hdr).await.map_err(CoreError::net)?;
        let mut buf = vec![0u8; payload_len(hdr)?];
        rx.read_exact(&mut buf).await.map_err(CoreError::net)?;
        Ok(decode_payload::<T>(&buf)?)
    })
    .await
    .map_err(|_| CoreError::Transfer(TransferError::Timeout))?
}
