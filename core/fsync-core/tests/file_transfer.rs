//! file/1: verified streaming, tamper detection, resume, change detection.

mod common;

use common::engine::{group, pseudo_random};
use common::files::{hash_file, offer_file, part_len, write_src};
use fsync_core::{
    CoreError, EngineConfig, EngineEvent, TransferError,
    clip::{ClipInput, FileInput, PublishOpts},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[tokio::test(flavor = "multi_thread")]
async fn fetch_200_mib_matches_hash_and_lands_in_dest() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    let src = write_src(ps[0].dir.path(), "big.bin", 200 << 20, 11);
    let want = hash_file(&src);
    let (id, f) = offer_file(&mut ps, &src).await;
    assert_eq!(f.hash, want);
    let dest = ps[1].dir.path().join("downloads");
    std::fs::create_dir(&dest).unwrap();
    let t = Instant::now();
    let path = ps[1]
        .engine
        .fetch_file_to(id, f.file_id, &dest)
        .await
        .unwrap();
    let dt = t.elapsed();
    println!(
        "200 MiB file/1 fetch: {dt:?} = {:.0} MiB/s",
        200.0 / dt.as_secs_f64()
    );
    assert_eq!(path, dest.canonicalize().unwrap().join("big.bin"));
    assert_eq!(hash_file(&path), want);
    // Never clobbers: the second copy gets a fresh name and the first stays intact.
    let again = ps[1]
        .engine
        .fetch_file_to(id, f.file_id, &dest)
        .await
        .unwrap();
    assert_eq!(again, dest.canonicalize().unwrap().join("big (1).bin"));
    assert_eq!(hash_file(&path), want);
    assert_eq!(hash_file(&again), want);
}

#[tokio::test(flavor = "multi_thread")]
async fn tampered_chunk_is_an_integrity_error() {
    let cfg = EngineConfig {
        serve_hook: Some(Arc::new(|off, data| {
            if off <= (5 << 20) && (5 << 20) < off + data.len() as u64 {
                data[(5 << 20) - off as usize] ^= 0x01;
            }
        })),
        ..EngineConfig::default()
    };
    let mut ps = group(vec![cfg, EngineConfig::default()]).await;
    let src = write_src(ps[0].dir.path(), "t.bin", 8 << 20, 5);
    let (id, f) = offer_file(&mut ps, &src).await;
    let r = ps[1].engine.fetch_file(id, f.file_id).await;
    assert!(
        matches!(r, Err(CoreError::Transfer(TransferError::Integrity))),
        "{r:?}"
    );
    // Nothing unverified became a complete blob.
    let done = ps[1]
        .blobs_dir()
        .join(blake3::Hash::from_bytes(f.hash).to_hex().as_str());
    assert!(!done.exists());
    // Verified prefix is below the tampered group.
    assert!(part_len(&ps[1], &f.hash) <= 5 << 20);
}

#[tokio::test(flavor = "multi_thread")]
async fn abort_mid_transfer_then_resume_fetches_only_the_rest() {
    let slow = Arc::new(AtomicBool::new(true));
    let s2 = slow.clone();
    let cfg = EngineConfig {
        serve_hook: Some(Arc::new(move |_, _| {
            if s2.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(40));
            }
        })),
        ..EngineConfig::default()
    };
    let mut ps = group(vec![cfg, EngineConfig::default()]).await;
    let size = 64usize << 20;
    let src = write_src(ps[0].dir.path(), "r.bin", size, 9);
    let (id, f) = offer_file(&mut ps, &src).await;
    let eng = ps[1].engine.clone();
    let task = tokio::spawn(async move { eng.fetch_file(id, f.file_id).await });
    let t0 = Instant::now();
    while part_len(&ps[1], &f.hash) < (6 << 20) {
        assert!(t0.elapsed() < Duration::from_secs(20), "no progress");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    task.abort();
    let _ = task.await;
    slow.store(false, Ordering::Relaxed);
    let kept = part_len(&ps[1], &f.hash);
    assert!(kept >= 6 << 20 && kept < size as u64, "kept {kept}");
    let st = ps[1].engine.fetch_file(id, f.file_id).await.unwrap();
    println!("resume: kept {kept}, stats {st:?}");
    assert_eq!(st.size, size as u64);
    assert!(st.resumed_from > 0 && st.resumed_from % 16384 == 0);
    assert!(st.resumed_from <= kept);
    assert_eq!(st.bytes_received, st.size - st.resumed_from);
    // Completed blob is byte-identical.
    let dest = ps[1].dir.path().join("out");
    std::fs::create_dir(&dest).unwrap();
    let p = ps[1]
        .engine
        .fetch_file_to(id, f.file_id, &dest)
        .await
        .unwrap();
    assert_eq!(hash_file(&p), f.hash);
}

#[tokio::test(flavor = "multi_thread")]
async fn source_modified_after_offer_is_changed() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    let src = write_src(ps[0].dir.path(), "c.bin", 3 << 20, 2);
    let (id, f) = offer_file(&mut ps, &src).await;
    std::fs::write(&src, pseudo_random((3 << 20) + 1, 99)).unwrap();
    let r = ps[1].engine.fetch_file(id, f.file_id).await;
    assert!(
        matches!(r, Err(CoreError::Transfer(TransferError::Changed))),
        "{r:?}"
    );
    // Same size, new content and mtime: also caught.
    let src2 = write_src(ps[0].dir.path(), "d.bin", 1 << 20, 3);
    let (id2, f2) = offer_file(&mut ps, &src2).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    std::fs::write(&src2, pseudo_random(1 << 20, 4)).unwrap();
    let r = ps[1].engine.fetch_file(id2, f2.file_id).await;
    assert!(
        matches!(r, Err(CoreError::Transfer(TransferError::Changed))),
        "{r:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn expired_outbox_entry_is_not_found() {
    let cfg = EngineConfig {
        outbox_ttl: Duration::from_millis(300),
        outbox_grace: Duration::from_millis(100),
        ..EngineConfig::default()
    };
    let mut ps = group(vec![cfg, EngineConfig::default()]).await;
    let src = write_src(ps[0].dir.path(), "e.bin", 1 << 20, 2);
    let (id, f) = offer_file(&mut ps, &src).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let r = ps[1].engine.fetch_file(id, f.file_id).await;
    assert!(
        matches!(r, Err(CoreError::Transfer(TransferError::NotFound))),
        "{r:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn push_flag_surfaces_incoming_files() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    let src = write_src(ps[0].dir.path(), "p.bin", 1000, 2);
    let b = ps[1].nid();
    ps[0]
        .engine
        .publish_clip(
            ClipInput::files(vec![FileInput::new(&src)]),
            PublishOpts {
                targets: Some(vec![b]),
                push: true,
            },
        )
        .await
        .unwrap();
    let n = ps[1]
        .wait(|e| match e {
            EngineEvent::IncomingFiles { offer, .. } => Some(offer.files.len()),
            _ => None,
        })
        .await;
    assert_eq!(n, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn source_vanished_reports_changed_or_not_found() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    let src = write_src(ps[0].dir.path(), "g.bin", 1 << 20, 1);
    let (id, f) = offer_file(&mut ps, &src).await;
    std::fs::remove_file(&src).unwrap();
    let r = ps[1].engine.fetch_file(id, f.file_id).await;
    assert!(
        matches!(
            r,
            Err(CoreError::Transfer(
                TransferError::Changed | TransferError::NotFound
            ))
        ),
        "{r:?}"
    );
}
