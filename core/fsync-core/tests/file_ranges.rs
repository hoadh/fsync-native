//! file/1 ranged reads (the Windows virtual-file path) and the 1 GiB benchmark.

mod common;

use common::engine::group;
use common::files::{offer_file, write_src};
use fsync_core::{
    CoreError, EngineConfig, EngineEvent, TransferError,
    clip::{ClipInput, PublishOpts},
};
use std::{sync::Arc, time::Instant};

#[tokio::test(flavor = "multi_thread")]
async fn ranged_reads_random_and_sequential_are_correct() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    let size = 24usize << 20;
    let src = write_src(ps[0].dir.path(), "v.bin", size, 77);
    let bytes = std::fs::read(&src).unwrap();
    let (id, f) = offer_file(&mut ps, &src).await;
    let e = &ps[1].engine;
    // Random access, unaligned, across window boundaries.
    let mut s = 12345u64;
    for _ in 0..40 {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let off = (s % size as u64) as usize;
        let len = 1 + ((s >> 20) % 300_000) as usize;
        let got = e
            .read_file_range(id, f.file_id, off as u64, len)
            .await
            .unwrap();
        let end = (off + len).min(size);
        assert_eq!(got, &bytes[off..end], "off {off} len {len}");
    }
    // Reads at/after EOF are empty, never an error.
    assert!(
        e.read_file_range(id, f.file_id, size as u64, 10)
            .await
            .unwrap()
            .is_empty()
    );
    // Sequential scan with prefetch.
    let t = Instant::now();
    let mut off = 0usize;
    while off < size {
        let got = e
            .read_file_range(id, f.file_id, off as u64, 64 << 10)
            .await
            .unwrap();
        assert!(!got.is_empty());
        assert_eq!(got, &bytes[off..off + got.len()]);
        off += got.len();
    }
    println!("sequential 24 MiB in 64 KiB reads: {:?}", t.elapsed());
    // Blocking flavour, from a plain thread as the shell extension would.
    let eng = e.clone();
    let got = tokio::task::spawn_blocking(move || {
        eng.read_file_range_blocking(id, f.file_id, 5_000_000, 1000)
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(got, &bytes[5_000_000..5_001_000]);
}

#[tokio::test(flavor = "multi_thread")]
async fn tampered_range_read_fails_integrity() {
    let cfg = EngineConfig {
        serve_hook: Some(Arc::new(|_, data| {
            if let Some(b) = data.first_mut() {
                *b ^= 0xff;
            }
        })),
        ..EngineConfig::default()
    };
    let mut ps = group(vec![cfg, EngineConfig::default()]).await;
    let src = write_src(ps[0].dir.path(), "w.bin", 2 << 20, 1);
    let (id, f) = offer_file(&mut ps, &src).await;
    let r = ps[1]
        .engine
        .read_file_range(id, f.file_id, 100_000, 10)
        .await;
    assert!(
        matches!(r, Err(CoreError::Transfer(TransferError::Integrity))),
        "{r:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cold_sequential_read_streams_whole_file() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    let size = 40usize << 20;
    let src = write_src(ps[0].dir.path(), "s.bin", size, 31);
    let bytes = std::fs::read(&src).unwrap();
    let (id, f) = offer_file(&mut ps, &src).await;
    let t = Instant::now();
    let mut off = 0usize;
    while off < size {
        let got = ps[1]
            .engine
            .read_file_range(id, f.file_id, off as u64, 256 << 10)
            .await
            .unwrap();
        assert!(!got.is_empty());
        assert_eq!(got, &bytes[off..off + got.len()]);
        off += got.len();
    }
    println!("cold sequential 40 MiB in 256 KiB reads: {:?}", t.elapsed());
}

/// `cargo test --release -p fsync-core --test file_transfer bench -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "1 GiB loopback benchmark"]
async fn bench_1_gib_loopback_and_clip_latency() {
    let mut ps = group(vec![EngineConfig::default(); 2]).await;
    let size = 1usize << 30;
    let src = write_src(ps[0].dir.path(), "gib.bin", size, 123);
    let t = Instant::now();
    let (id, f) = offer_file(&mut ps, &src).await;
    println!("BENCH offer (hash+outboard) 1 GiB: {:?}", t.elapsed());
    let t = Instant::now();
    let st = ps[1].engine.fetch_file(id, f.file_id).await.unwrap();
    let dt = t.elapsed();
    println!(
        "BENCH 1 GiB fetch: {dt:?} = {:.0} MiB/s ({:.2} Gbit/s), received {}",
        1024.0 / dt.as_secs_f64(),
        8.0 * 1024.0 / 1000.0 / dt.as_secs_f64() * 1.048576,
        st.bytes_received
    );
    let mut ts = vec![];
    for i in 0..20 {
        let t = Instant::now();
        ps[0]
            .engine
            .publish_clip(ClipInput::text(&format!("lat {i}")), PublishOpts::default())
            .await
            .unwrap();
        ps[1]
            .wait(|e| matches!(e, EngineEvent::RemoteClip { offer, .. } if offer.preview.as_deref() == Some(&format!("lat {i}"))).then_some(()))
            .await;
        ts.push(t.elapsed());
    }
    ts.sort();
    println!(
        "BENCH clip latency median {:?} p95 {:?} max {:?}",
        ts[10], ts[18], ts[19]
    );
}
