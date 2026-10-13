//! Turns app input into a `ClipOffer` plus the outbox entry that can serve its fetches.

use super::{input::ClipInput, limits};
use crate::{
    blobs::BlobStore,
    config::EngineConfig,
    error::{CoreError, Result},
    file::{Entry, Served, Source, digest_read},
};
use fsync_proto::{
    ClipOffer, FileMeta, NodeId, RepMeta,
    clip::{CLIP_VERSION, MAX_PREVIEW_CHARS, MAX_REPS},
    names::{validate_file_name, validate_rel_path},
};
use std::{
    collections::HashSet,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Hash everything (off the async threads), spill big reps, pick inline reps.
pub async fn build_entry(
    input: ClipInput,
    cfg: &EngineConfig,
    max_clip: u64,
    store: &Arc<dyn BlobStore>,
    me: NodeId,
    push: bool,
    persist: bool,
) -> Result<Entry> {
    if input.reps.len() > MAX_REPS {
        return Err(CoreError::Invalid("too many representations"));
    }
    let total = input.reps.iter().map(|r| r.bytes.len() as u64).sum();
    limits::check_total(total, max_clip)?;
    let (store, spill) = (store.clone(), cfg.spill_threshold);
    tokio::task::spawn_blocking(move || build(input, store, spill, me, push, persist))
        .await
        .map_err(|_| CoreError::Closed)?
}

fn build(
    input: ClipInput,
    store: Arc<dyn BlobStore>,
    spill: usize,
    me: NodeId,
    push: bool,
    persist: bool,
) -> Result<Entry> {
    let input_sensitive = input.sensitive;
    let sizes: Vec<usize> = input.reps.iter().map(|r| r.bytes.len()).collect();
    let fits = limits::choose_inline(&sizes);
    let (mut metas, mut inline, mut reps) = (vec![], vec![], vec![]);
    for (rep, inl) in input.reps.into_iter().zip(fits) {
        let d = digest_read(rep.bytes.as_slice())?;
        metas.push(RepMeta {
            mime: rep.mime,
            size: d.size,
            hash: d.hash,
        });
        inline.push(inl.then(|| rep.bytes.clone()));
        // `persist`: history needs every rep in the store, not just the spilled ones.
        if persist || rep.bytes.len() > spill {
            store.put(&d.hash, &rep.bytes)?;
        }
        let source = if rep.bytes.len() > spill {
            Source::Blob(d.hash)
        } else {
            Source::Mem(Arc::new(rep.bytes))
        };
        reps.push(Arc::new(Served::from_digest(source, d)));
    }
    let (mut files, mut served) = (vec![], vec![]);
    let mut seen = HashSet::new();
    for (i, f) in input.files.into_iter().enumerate() {
        validate_rel_path(&f.rel_path)?;
        let name = f.rel_path.rsplit('/').next().unwrap_or_default().to_owned();
        validate_file_name(&name)?;
        if !seen.insert(f.rel_path.clone()) {
            return Err(CoreError::Invalid("duplicate rel_path"));
        }
        let s = Served::snapshot(f.path)?;
        let mtime_ms = match s.source {
            Source::Path { mtime_ms, .. } => mtime_ms,
            _ => 0,
        };
        files.push(FileMeta {
            file_id: u32::try_from(i).map_err(|_| CoreError::Invalid("too many files"))?,
            name,
            rel_path: f.rel_path,
            size: s.size,
            hash: s.hash,
            mtime_ms,
        });
        served.push(Arc::new(s));
    }
    let offer = ClipOffer {
        v: CLIP_VERSION,
        item_id: Uuid::now_v7(),
        origin: me,
        ts_ms: now_ms(),
        kind: input.kind,
        reps: metas,
        inline,
        files,
        preview: input
            .preview
            .map(|p| p.chars().take(MAX_PREVIEW_CHARS).collect()),
        push,
        sensitive: input_sensitive,
    };
    offer.validate()?; // our own output must pass what receivers check
    Ok(Entry {
        offer: Arc::new(offer),
        reps,
        files: served,
        offered_to: HashSet::new(),
        created: Instant::now(),
        superseded: None,
    })
}
