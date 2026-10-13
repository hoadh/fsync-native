//! spike/0 protocol: 1 op byte + u64 len on a bi stream. 'E' echo-ack, 'U' upload `len` bytes then ack.
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use n0_future::StreamExt;

#[derive(Debug, Clone)]
pub struct Spike;

impl ProtocolHandler for Spike {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        // Remote identity comes from the TLS handshake; no extra auth needed to read it.
        let remote = conn.remote_id();
        println!("ACCEPT remote={remote} alpn={}", String::from_utf8_lossy(conn.alpn()));
        let mut ev = conn.path_events();
        tokio::spawn(async move {
            while let Some(e) = ev.next().await { println!("SRV_PATH {e:?}"); }
        });
        loop {
            let (mut tx, mut rx) = match conn.accept_bi().await {
                Ok(s) => s,
                Err(_) => return Ok(()), // closed by peer
            };
            let mut hdr = [0u8; 9];
            rx.read_exact(&mut hdr).await.map_err(AcceptError::from_err)?;
            let len = u64::from_be_bytes(hdr[1..9].try_into().unwrap());
            if hdr[0] == b'U' {
                let mut left = len;
                let mut buf = vec![0u8; 1 << 20];
                while left > 0 {
                    match rx.read(&mut buf).await.map_err(AcceptError::from_err)? {
                        Some(n) => left = left.saturating_sub(n as u64),
                        None => break,
                    }
                }
            }
            tx.write_all(&[1]).await.map_err(AcceptError::from_err)?;
            tx.finish().map_err(AcceptError::from_err)?;
        }
    }
}
