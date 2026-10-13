use super::*;

#[test]
fn blocks_after_limit_and_isolates_keys() {
    let rl = RateLimiter::default();
    let w = Duration::from_secs(60);
    for _ in 0..3 {
        assert!(rl.check("a", 3, w).is_ok());
    }
    assert!(matches!(
        rl.check("a", 3, w),
        Err(ApiError::RateLimited { .. })
    ));
    assert!(rl.check("b", 3, w).is_ok());
}

#[test]
fn window_expires() {
    let rl = RateLimiter::default();
    let w = Duration::from_millis(30);
    assert!(rl.check("a", 1, w).is_ok());
    assert!(rl.check("a", 1, w).is_err());
    std::thread::sleep(Duration::from_millis(40));
    assert!(rl.check("a", 1, w).is_ok());
}

#[test]
fn map_is_hard_capped_and_prefers_dropping_expired_keys() {
    let rl = RateLimiter::with_max_keys(50);
    let long = Duration::from_secs(3600);
    for i in 0..500 {
        assert!(rl.check(&format!("k{i}"), 5, long).is_ok());
        assert!(rl.len() <= 50);
    }
    // Expired keys are reclaimed before live ones are evicted.
    let rl = RateLimiter::with_max_keys(3);
    for k in ["a", "b"] {
        rl.check(k, 1, Duration::from_millis(10)).unwrap();
    }
    rl.check("live", 1, long).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    rl.check("new", 1, long).unwrap();
    assert!(rl.check("live", 1, long).is_err(), "live key must survive");
}

#[test]
fn short_window_keys_do_not_evict_long_window_keys() {
    // Regression: pruning with the caller's window used to drop other keys' state.
    let rl = RateLimiter::default();
    let day = Duration::from_secs(86_400);
    rl.check("slow", 1, day).unwrap();
    for i in 0..50 {
        rl.check(&format!("fast{i}"), 1, Duration::from_millis(1))
            .unwrap();
    }
    std::thread::sleep(Duration::from_millis(5));
    rl.check("trigger", 1, Duration::from_millis(1)).unwrap();
    assert!(rl.check("slow", 1, day).is_err());
}

#[test]
fn full_map_scans_for_expired_keys_at_most_once_per_second() {
    let rl = RateLimiter::with_max_keys(2);
    let (short, long) = (Duration::from_millis(10), Duration::from_secs(3600));
    rl.check("k1", 1, short).unwrap();
    rl.check("k2", 1, short).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    rl.check("k3", 1, short).unwrap(); // scan: k1, k2 reclaimed
    rl.check("k4", 1, long).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    // k3 expired, but the map is full and the scan is throttled: a victim is evicted.
    rl.check("k5", 1, long).unwrap();
    assert_eq!(rl.len(), 2);
}

#[test]
fn peek_records_nothing_and_record_counts() {
    let rl = RateLimiter::default();
    let w = Duration::from_secs(60);
    for _ in 0..5 {
        assert!(rl.peek("a", 1, w).is_ok());
    }
    assert!(rl.is_empty());
    rl.record("a", w);
    assert!(rl.peek("a", 1, w).is_err());
    assert!(rl.check("a", 1, w).is_err());
}
