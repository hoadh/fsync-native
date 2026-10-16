//! send / fetch / bench.

use crate::{
    ctx::{Ctx, emit},
    events::Ev,
};
use anyhow::{Result, bail};
use fsync_core::service::{FsyncError, TransferDirection, TransferState};
use serde_json::json;
use std::{
    io::{Read, Write},
    path::Path,
    time::{Duration, Instant},
};

fn mib_s(bytes: u64, secs: f64) -> f64 {
    bytes as f64 / (1 << 20) as f64 / secs.max(1e-6)
}

/// Pushes `paths` to `node`, retrying while the peer has not taken the offer yet.
pub fn send(c: &Ctx, node: &str, paths: Vec<String>, wait: u64) -> Result<()> {
    c.online(true)?;
    let end = Instant::now() + Duration::from_secs(wait);
    let dev = c.wait_online(node, Duration::from_secs(wait))?;
    let t0 = Instant::now();
    let id = loop {
        match c.svc.send_files(&dev.node_id, paths.clone()) {
            Ok(id) => break id,
            Err(FsyncError::PeerOffline) if Instant::now() < end => {
                std::thread::sleep(Duration::from_secs(1))
            }
            Err(e) => return Err(e.into()),
        }
    };
    emit(json!({"event": "offered", "transfer_id": id}));
    // Remember the latest tier seen while the transfer runs: the peer may close right after Done.
    let mut tier = c.path_tier(&dev.node_id);
    let total = c.wait_event(Instant::now() + Duration::from_secs(3600), |ev| match ev {
        Ev::Transfer(t) if t.transfer_id == id && t.direction == TransferDirection::Send => {
            tier = c.path_tier(&dev.node_id).or(tier);
            match t.state {
                TransferState::Done => Some(Ok(t.bytes_total)),
                TransferState::Failed | TransferState::Cancelled => Some(Err(anyhow::anyhow!(
                    "send {:?}: {}",
                    t.state,
                    t.message.unwrap_or_default()
                ))),
                _ => None,
            }
        }
        _ => None,
    })??;
    let secs = t0.elapsed().as_secs_f64();
    let path = c.path_tier(&dev.node_id).or(tier);
    emit(
        json!({"event": "sent", "bytes": total, "secs": secs, "mib_s": mib_s(total, secs), "via": format!("{:?}", c.resolve(&dev.node_id)?.status), "path": path}),
    );
    Ok(())
}

fn blake3_hex(p: &Path) -> Result<String> {
    let mut f = std::fs::File::open(p)?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            return Ok(h.finalize().to_hex().to_string());
        }
        h.update(&buf[..n]);
    }
}

pub fn fetch(c: &Ctx, out: &Path, count: u32, timeout: u64) -> Result<()> {
    c.online(true)?;
    crate::session::ensure_dir(out)?;
    emit(json!({"event": "ready", "node_id": c.svc.node_id()}));
    let deadline = Instant::now() + Duration::from_secs(timeout);
    for _ in 0..count {
        let offer = c.wait_event(deadline, |ev| match ev {
            Ev::Files(o) => Some(o),
            _ => None,
        })?;
        for f in &offer.files {
            let t0 = Instant::now();
            // The sender may close its side right after Done, so remember the latest tier seen
            // before and during the transfer instead of sampling only at the end.
            let mut tier = c.path_tier(&offer.from);
            let id = c
                .svc
                .fetch_file_to(&offer.item_id, f.file_id, &out.to_string_lossy())?;
            let (path, bytes) =
                c.wait_event(Instant::now() + Duration::from_secs(3600), |ev| match ev {
                    Ev::Transfer(t)
                        if t.transfer_id == id && t.direction == TransferDirection::Receive =>
                    {
                        tier = c.path_tier(&offer.from).or(tier);
                        match t.state {
                            TransferState::Done => {
                                Some(Ok((t.path.unwrap_or_default(), t.bytes_total)))
                            }
                            TransferState::Failed | TransferState::Cancelled => {
                                Some(Err(anyhow::anyhow!(
                                    "receive {:?}: {}",
                                    t.state,
                                    t.message.unwrap_or_default()
                                )))
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                })??;
            let secs = t0.elapsed().as_secs_f64();
            // `path` is the saved file here, so the network path is `path_tier`.
            let path_tier = c.path_tier(&offer.from).or(tier);
            emit(
                json!({"event": "fetched", "path": path, "path_tier": path_tier, "bytes": bytes, "secs": secs,
                "mib_s": mib_s(bytes, secs), "blake3": blake3_hex(Path::new(&path))?}),
            );
        }
    }
    Ok(())
}

pub fn bench(c: &Ctx, node: &str, mib: u64, wait: u64) -> Result<()> {
    if mib == 0 {
        bail!("--mib must be > 0");
    }
    let path = c.dir.join(format!("bench-{mib}mib.bin"));
    {
        let mut f = std::fs::File::create(&path)?;
        let mut block = vec![0u8; 1 << 20];
        for i in 0..mib {
            getrandom::fill(&mut block[..4096]).map_err(|e| anyhow::anyhow!("rng: {e}"))?;
            block[4096] = i as u8;
            f.write_all(&block)?;
        }
        f.sync_all()?;
    }
    let r = send(c, node, vec![path.to_string_lossy().into_owned()], wait);
    // Best effort: a leftover temp file is harmless.
    let _ = std::fs::remove_file(&path);
    r
}
