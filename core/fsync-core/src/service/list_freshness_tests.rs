use super::*;
use crate::error::ApiError;
use std::time::Duration;

const DAY: u64 = 24 * 3600;

fn unreachable() -> CoreError {
    CoreError::Unreachable("connection refused".into())
}

/// A history database file whose mtime is `age` seconds ago.
fn db_aged(dir: &Path, age: u64) {
    let f = std::fs::File::create(dir.join(DB_FILE)).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(age))
        .unwrap();
}

fn write_stamp(dir: &Path, t: u64) {
    std::fs::write(dir.join(FILE), t.to_string()).unwrap();
}

#[test]
fn unreachable_within_the_bound_is_allowed() {
    assert_eq!(age_if_allowed(&unreachable(), 100, 100 + DAY), Some(DAY));
    assert!(age_if_allowed(&unreachable(), 0, MAX_AGE_SECS).is_some());
}

#[test]
fn past_the_bound_is_refused() {
    assert_eq!(age_if_allowed(&unreachable(), 0, MAX_AGE_SECS + 1), None);
}

#[test]
fn suspicious_errors_are_never_allowed() {
    let api = CoreError::Api(ApiError {
        status: 503,
        code: "x".into(),
        message: String::new(),
    });
    for e in [
        CoreError::Http("error sending request".into()),
        CoreError::Http("bad response body: eof".into()),
        CoreError::Http("response too large".into()),
        CoreError::Net("x".into()),
        api,
        CoreError::AuthExpired,
    ] {
        assert_eq!(age_if_allowed(&e, 10, 11), None, "{e:?}");
    }
}

#[test]
fn stamp_round_trips_and_garbage_is_no_record() {
    let dir = tempfile::tempdir().unwrap();
    let f = ListFreshness::new(dir.path());
    let now = unix_now();
    assert_eq!(f.last(now), None);
    f.stamp();
    assert!(f.last(unix_now()).unwrap() >= now);
    std::fs::write(dir.path().join(FILE), "garbage").unwrap();
    assert_eq!(f.last(now), None);
}

#[test]
fn legacy_install_ages_from_the_database_mtime() {
    let now = unix_now();
    let recent = tempfile::tempdir().unwrap();
    db_aged(recent.path(), 2 * DAY);
    let f = ListFreshness::new(recent.path());
    let age = f.offline_ok(&unreachable(), now).expect("recent db");
    assert!((2 * DAY..2 * DAY + 60).contains(&age), "{age}");
    assert_eq!(f.last(now), None, "the fallback must not start a new clock");

    let old = tempfile::tempdir().unwrap();
    db_aged(old.path(), MAX_AGE_SECS + DAY);
    let f = ListFreshness::new(old.path());
    assert_eq!(f.offline_ok(&unreachable(), now), None);
}

#[test]
fn no_stamp_and_no_database_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let f = ListFreshness::new(dir.path());
    assert_eq!(f.offline_ok(&unreachable(), unix_now()), None);
}

#[test]
fn a_future_stamp_is_invalid_and_falls_back() {
    let now = unix_now();
    let dir = tempfile::tempdir().unwrap();
    let f = ListFreshness::new(dir.path());
    write_stamp(dir.path(), now + 30 * DAY);
    assert_eq!(f.last(now), None);
    // No database to fall back on: refused, not "age 0".
    assert_eq!(f.offline_ok(&unreachable(), now), None);
    // With a stale database the fallback applies, and it is the stale one.
    db_aged(dir.path(), MAX_AGE_SECS + DAY);
    assert_eq!(f.offline_ok(&unreachable(), now), None);
    db_aged(dir.path(), DAY);
    assert!(f.offline_ok(&unreachable(), now).unwrap() >= DAY);
}

#[test]
fn small_skew_is_tolerated_and_just_over_is_not() {
    let now = unix_now();
    let dir = tempfile::tempdir().unwrap();
    let f = ListFreshness::new(dir.path());
    write_stamp(dir.path(), now + MAX_SKEW_SECS);
    assert_eq!(f.offline_ok(&unreachable(), now), Some(0));
    write_stamp(dir.path(), now + MAX_SKEW_SECS + 1);
    assert_eq!(f.last(now), None);
    assert_eq!(f.offline_ok(&unreachable(), now), None);
}
