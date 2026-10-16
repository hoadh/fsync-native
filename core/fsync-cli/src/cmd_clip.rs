//! clip put / clip watch.

use crate::{
    ctx::{Ctx, emit},
    events::Ev,
};
use anyhow::{Result, bail};
use fsync_core::service::{ClipRep, FsyncError, LocalClip};
use serde_json::json;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Publishes once other devices are reachable. Every publish is a new item in history, so
/// the wait happens first and a failed delivery is retried only `MAX_ATTEMPTS` times.
pub fn put(c: &Ctx, text: &str, wait: u64) -> Result<()> {
    const MAX_ATTEMPTS: u32 = 3;
    c.online(true)?;
    let end = Instant::now() + Duration::from_secs(wait);
    let started = Instant::now();
    c.settle_presence(Duration::from_secs(wait));
    let settle_ms = started.elapsed().as_millis() as u64;
    let first_publish_ms = unix_ms();
    for attempt in 1..=MAX_ATTEMPTS {
        let clip = LocalClip {
            reps: vec![ClipRep {
                mime: "text/plain".into(),
                data: text.as_bytes().to_vec(),
            }],
            files: vec![],
            preview: None,
            sensitive: false,
        };
        let p = c.svc.publish_clip(clip)?;
        if !p.delivered.is_empty() {
            emit(
                json!({"event": "published", "item_id": p.item_id, "delivered": p.delivered, "refused": p.refused,
                "attempts": attempt, "settle_ms": settle_ms, "first_publish_ms": first_publish_ms}),
            );
            return Ok(());
        }
        // Every peer that did not take it no longer trusts us: retrying cannot help.
        if p.failed.is_empty() && !p.refused.is_empty() {
            bail!(FsyncError::NotPaired);
        }
        if attempt == MAX_ATTEMPTS || Instant::now() >= end {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    bail!(FsyncError::PeerOffline)
}

pub fn watch(c: &Ctx, as_json: bool, count: u32, timeout: u64) -> Result<()> {
    c.online(true)?;
    emit(json!({"event": "ready", "node_id": c.svc.node_id()}));
    let deadline = Instant::now() + Duration::from_secs(timeout);
    let mut seen = 0u32;
    while count == 0 || seen < count {
        let item = c.wait_event(deadline, |ev| match ev {
            Ev::Clip(i) => Some(i),
            _ => None,
        })?;
        let text = match item.reps.first() {
            Some(r) if r.mime == "text/plain" => {
                Some(String::from_utf8_lossy(&c.svc.fetch_rep(&item.item_id, 0)?).into_owned())
            }
            _ => None,
        };
        let v = json!({"event": "clip", "item_id": item.item_id, "from": item.from, "from_name": item.from_name,
            "kind": format!("{:?}", item.kind).to_lowercase(), "total_size": item.total_size, "text": text, "ts_ms": unix_ms()});
        if as_json {
            emit(v);
        } else {
            println!("{}", text.unwrap_or_default());
        }
        seen += 1;
    }
    Ok(())
}
