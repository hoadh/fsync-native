//! Keeps a recent [`LocalSubnets`] snapshot for iroh's path selection without blocking on the OS.

use super::local_subnets::LocalSubnets;
use std::{
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// Interfaces change rarely (Wi-Fi join, cable plugged); re-read them at most this often.
const REFRESH_EVERY: Duration = Duration::from_secs(5);

/// A recent [`LocalSubnets`] snapshot for iroh's path selection. [`SubnetCache::get`] never blocks
/// on the OS: a stale snapshot is returned at once while one background thread re-reads it.
#[derive(Debug, Clone)]
pub struct SubnetCache {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    snapshot: RwLock<(Instant, Arc<LocalSubnets>)>,
    refreshing: AtomicBool,
    read: fn() -> LocalSubnets,
    every: Duration,
}

impl Default for SubnetCache {
    fn default() -> Self {
        Self::with_reader(LocalSubnets::read, REFRESH_EVERY)
    }
}

impl SubnetCache {
    /// Reads once now (call at node build time, not inside path selection).
    pub fn with_reader(read: fn() -> LocalSubnets, every: Duration) -> Self {
        let first = (
            Instant::now(),
            Arc::new(std::panic::catch_unwind(read).unwrap_or_default()),
        );
        Self {
            inner: Arc::new(Inner {
                snapshot: RwLock::new(first),
                refreshing: AtomicBool::new(false),
                read,
                every,
            }),
        }
    }

    /// The latest snapshot; schedules a background re-read when it is older than the interval.
    pub fn get(&self) -> Arc<LocalSubnets> {
        let (at, nets) = {
            let snap = self
                .inner
                .snapshot
                .read()
                .unwrap_or_else(|e| e.into_inner());
            (snap.0, Arc::clone(&snap.1))
        };
        if at.elapsed() >= self.inner.every && !self.inner.refreshing.swap(true, Ordering::AcqRel) {
            let inner = Arc::clone(&self.inner);
            let spawned = std::thread::Builder::new()
                .name("fsync-subnets".into())
                .spawn(move || {
                    // A failed (panicking) read keeps the old snapshot but restamps it, so the
                    // next attempt waits a full interval instead of retrying on every select().
                    let fresh = std::panic::catch_unwind(inner.read).ok();
                    let mut snap = inner.snapshot.write().unwrap_or_else(|e| e.into_inner());
                    let nets = fresh.map(Arc::new).unwrap_or_else(|| Arc::clone(&snap.1));
                    *snap = (Instant::now(), nets);
                    drop(snap);
                    inner.refreshing.store(false, Ordering::Release);
                });
            if spawned.is_err() {
                self.inner.refreshing.store(false, Ordering::Release);
            }
        }
        nets
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    fn subnets(nets: &[&str]) -> LocalSubnets {
        LocalSubnets::from_nets(nets.iter().map(|n| n.parse().unwrap()))
    }

    #[test]
    fn panicking_refresh_does_not_freeze_the_cache() {
        use std::sync::atomic::AtomicUsize;
        static READS: AtomicUsize = AtomicUsize::new(0);
        fn reader() -> LocalSubnets {
            match READS.fetch_add(1, Ordering::SeqCst) {
                0 => LocalSubnets::default(),
                1 => panic!("interface table unreadable (expected in this test)"),
                _ => subnets(&["10.8.0.1/24"]),
            }
        }
        let cache = SubnetCache::with_reader(reader, Duration::ZERO);
        let probe: IpAddr = "10.8.0.9".parse().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !cache.get().contains(probe) {
            assert!(Instant::now() < deadline, "refresh stuck after a panic");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn failing_reads_back_off_for_an_interval() {
        use std::sync::atomic::AtomicUsize;
        static READS: AtomicUsize = AtomicUsize::new(0);
        fn reader() -> LocalSubnets {
            if READS.fetch_add(1, Ordering::SeqCst) > 0 {
                panic!("interface table unreadable (expected in this test)");
            }
            LocalSubnets::default()
        }
        let cache = SubnetCache::with_reader(reader, Duration::from_millis(50));
        let until = Instant::now() + Duration::from_millis(300);
        while Instant::now() < until {
            cache.get();
            std::thread::sleep(Duration::from_millis(1));
        }
        // ~6 intervals; without the restamp every get() would start a new failing read.
        assert!(
            READS.load(Ordering::SeqCst) <= 12,
            "reads: {}",
            READS.load(Ordering::SeqCst)
        );
    }

    #[test]
    fn get_never_blocks_and_refreshes_in_background() {
        use std::sync::atomic::AtomicUsize;
        static READS: AtomicUsize = AtomicUsize::new(0);
        fn reader() -> LocalSubnets {
            let n = READS.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                LocalSubnets::default()
            } else {
                subnets(&["10.9.0.1/24"])
            }
        }
        let cache = SubnetCache::with_reader(reader, Duration::ZERO);
        let probe: IpAddr = "10.9.0.7".parse().unwrap();
        // The first stale get returns the old snapshot immediately and starts one refresh.
        assert!(!cache.get().contains(probe));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !cache.get().contains(probe) {
            assert!(Instant::now() < deadline, "background refresh never landed");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn fresh_snapshot_is_not_reread() {
        use std::sync::atomic::AtomicUsize;
        static READS: AtomicUsize = AtomicUsize::new(0);
        fn reader() -> LocalSubnets {
            READS.fetch_add(1, Ordering::SeqCst);
            LocalSubnets::default()
        }
        let cache = SubnetCache::with_reader(reader, Duration::from_secs(60));
        cache.get();
        cache.get();
        assert_eq!(
            READS.load(Ordering::SeqCst),
            1,
            "only the construction read"
        );
    }
}
