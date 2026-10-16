//! Receiver side of file/1: open a verified session and read verified batches.

use super::group_tree::{Cv, Verifier, group_count};
use super::policy::{MAX_CV_BYTES, max_verifiable_size};
use crate::{
    blobs::{Hash, blocking},
    error::{CoreError, Result, TransferError},
    net::{
        Dialer,
        frames::{recv_frame, send_frame},
    },
};
use fsync_proto::{ALPN_FILE, FetchReq, FetchResp, FetchStatus, NodeId, file::CV_LEN};
use iroh::endpoint::RecvStream;
use std::{sync::Arc, time::Duration};

/// Limit for the handshake on a cached connection, which may be dead after a peer crash.
/// Generous enough for a slow relay (about 0.4 MiB/s, high RTT) or a sender busy with other streams.
const FIRST_ATTEMPT: Duration = Duration::from_secs(5);

/// A transfer stalls (error, not hang) after this long without progress.
pub const STALL: Duration = Duration::from_secs(30);

/// What the offer promised; the sender must prove exactly this.
#[derive(Clone, Copy, Debug)]
pub struct Expect {
    pub size: u64,
    pub hash: Hash,
}

pub struct Session {
    pub rx: RecvStream,
    pub verifier: Arc<Verifier>,
}

fn stalled<T>(r: std::result::Result<Result<T>, tokio::time::error::Elapsed>) -> Result<T> {
    r.map_err(|_| CoreError::Transfer(TransferError::Timeout))?
}

async fn read_exact(rx: &mut RecvStream, buf: &mut [u8]) -> Result<()> {
    stalled(
        tokio::time::timeout(STALL, async {
            rx.read_exact(buf).await.map_err(CoreError::net)
        })
        .await,
    )
}

/// Send `req` and read the response header. On a cached connection the first try is short; if the
/// path turns out dead (transport error or silence) the connection is dropped and the request is
/// repeated once on a fresh one. Protocol answers are never retried (the caller checks them).
async fn handshake(
    dialer: &Dialer,
    peer: &NodeId,
    req: &FetchReq,
) -> Result<(FetchResp, RecvStream)> {
    let (conn, reused) = dialer.connect_reporting_reuse(peer, ALPN_FILE).await?;
    let limit = if reused { FIRST_ATTEMPT } else { STALL };
    let rx_before = conn.stats().udp_rx.bytes;
    match request(&conn, req, limit).await {
        // Dead = silence past the deadline, or the connection itself is closed. Any other error
        // (stream refused, protocol) says the path works and is not retried.
        Err(e) if reused && is_dead_path(&conn, &e) => {
            dialer.abandon(peer, ALPN_FILE, &conn, rx_before);
            let conn = dialer.connect(peer, ALPN_FILE).await?;
            request(&conn, req, STALL).await
        }
        r => r,
    }
}

fn is_dead_path(conn: &iroh::endpoint::Connection, e: &CoreError) -> bool {
    matches!(e, CoreError::Transfer(TransferError::Timeout))
        || (matches!(e, CoreError::Net(_)) && conn.close_reason().is_some())
}

async fn request(
    conn: &iroh::endpoint::Connection,
    req: &FetchReq,
    wait: Duration,
) -> Result<(FetchResp, RecvStream)> {
    let (mut tx, mut rx) = conn.open_bi().await.map_err(CoreError::net)?;
    send_frame(&mut tx, req).await?;
    tx.finish().map_err(CoreError::net)?;
    let resp: FetchResp = recv_frame(&mut rx, wait).await?;
    Ok((resp, rx))
}

/// Send `req`, check the response against `exp`, and (unless `known`) read + verify the CV list.
pub async fn open(
    dialer: &Dialer,
    peer: &NodeId,
    mut req: FetchReq,
    exp: Expect,
    known: Option<Arc<Verifier>>,
) -> Result<Session> {
    req.no_cvs = known.is_some();
    let (resp, mut rx) = handshake(dialer, peer, &req).await?;
    match resp.status {
        FetchStatus::Ok => {}
        FetchStatus::Denied => return Err(TransferError::Denied.into()),
        FetchStatus::NotFound => return Err(TransferError::NotFound.into()),
        FetchStatus::Changed => return Err(TransferError::Changed.into()),
    }
    if resp.size != exp.size || resp.hash != exp.hash {
        return Err(TransferError::Integrity.into());
    }
    let verifier = match known {
        Some(v) => v,
        None => Arc::new(read_cvs(&mut rx, exp).await?),
    };
    Ok(Session { rx, verifier })
}

async fn read_cvs(rx: &mut RecvStream, exp: Expect) -> Result<Verifier> {
    let n = group_count(exp.size);
    if n.saturating_mul(CV_LEN as u64) > MAX_CV_BYTES {
        return Err(CoreError::TooLarge {
            size: exp.size,
            max: max_verifiable_size(),
        });
    }
    let mut cvs: Vec<Cv> = Vec::new();
    if n >= 2 {
        // Grow as bytes arrive: a lying size cannot make us allocate up front.
        let mut buf = vec![0u8; 2048 * CV_LEN];
        let mut left = n;
        while left > 0 {
            let k = usize::try_from(left).unwrap_or(2048).min(2048);
            read_exact(rx, &mut buf[..k * CV_LEN]).await?;
            cvs.extend_from_slice(buf[..k * CV_LEN].as_chunks::<CV_LEN>().0);
            left -= k as u64;
        }
    }
    Verifier::new(exp.hash, exp.size, cvs).ok_or(TransferError::Integrity.into())
}

/// Read `want` bytes starting at group-aligned `pos` and verify them before returning.
pub async fn next_batch(
    rx: &mut RecvStream,
    v: &Arc<Verifier>,
    pos: u64,
    want: usize,
) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; want];
    read_exact(rx, &mut buf).await?;
    let v = v.clone();
    let (buf, ok) = blocking(move || {
        let ok = v.check(pos, &buf);
        Ok((buf, ok))
    })
    .await?;
    if ok {
        Ok(buf)
    } else {
        Err(TransferError::Integrity.into())
    }
}
