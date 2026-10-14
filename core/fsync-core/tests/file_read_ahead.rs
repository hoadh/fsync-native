//! Lazy-paste read pattern (ADR 010): one reader issues 256 KiB reads strictly one at a time.
//! `serve_delay` stands in for the network round trip of every file/1 request.

mod common;

use common::engine::group;
use common::files::{offer_file, write_src};
use fsync_core::EngineConfig;
use std::time::{Duration, Instant};

const CHUNK: usize = 256 << 10;

/// Serial 256 KiB scan of the whole file; returns the elapsed time. Verifies every byte.
async fn scan(size: usize, rtt: Duration) -> Duration {
    let cfg = EngineConfig {
        serve_delay: Some(rtt),
        ..EngineConfig::default()
    };
    let mut ps = group(vec![cfg, EngineConfig::default()]).await;
    let src = write_src(ps[0].dir.path(), "ra.bin", size, 5);
    let bytes = std::fs::read(&src).unwrap();
    let (id, f) = offer_file(&mut ps, &src).await;
    let t = Instant::now();
    let mut off = 0usize;
    while off < size {
        let got = ps[1]
            .engine
            .read_file_range(id, f.file_id, off as u64, CHUNK)
            .await
            .unwrap();
        assert!(!got.is_empty());
        assert_eq!(got, &bytes[off..off + got.len()]);
        off += got.len();
    }
    t.elapsed()
}

#[tokio::test(flavor = "multi_thread")]
async fn serial_small_reads_overlap_round_trips() {
    let (size, rtt) = (64usize << 20, Duration::from_millis(100));
    // The zero-delay scan is the CPU cost (hash checks, copies; large in debug builds).
    let cpu = scan(size, Duration::ZERO).await;
    let dt = scan(size, rtt).await;
    // Fetching one 4 MiB window per round trip adds at least 16 x rtt on top of the CPU cost.
    let serial_floor = rtt * (size as u32 >> 22);
    println!(
        "64 MiB, 256 KiB serial reads: rtt 0 {cpu:?}, rtt 100 ms {dt:?}, serial floor +{serial_floor:?}"
    );
    assert!(
        dt < cpu + serial_floor.mul_f32(0.6),
        "{dt:?} vs {cpu:?} + {serial_floor:?}"
    );
}

/// `cargo test --release -p fsync-core --test file_read_ahead bench -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "200 MiB benchmark"]
async fn bench_200_mib_serial_256k_reads() {
    for ms in [0u64, 50, 100] {
        let dt = scan(200 << 20, Duration::from_millis(ms)).await;
        println!(
            "BENCH 200 MiB / 256 KiB serial reads, rtt {ms} ms: {dt:?} = {:.1} MiB/s",
            200.0 / dt.as_secs_f64()
        );
    }
}
