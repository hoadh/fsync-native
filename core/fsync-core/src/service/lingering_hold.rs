//! Bookkeeping for the background holder of `engine.lock`, so a `start` that finds the lock
//! taken by our own still-releasing engine can say so (and for how long) instead of reporting
//! some other process.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub(super) struct LingeringHold(Arc<Mutex<Option<Instant>>>);

impl LingeringHold {
    /// A holder starts; it gives up at the latest after `max_hold`.
    pub(super) fn begin(&self, max_hold: Duration) {
        if let Ok(mut g) = self.0.lock() {
            *g = Some(Instant::now() + max_hold);
        }
    }

    /// The holder released the lock.
    pub(super) fn end(&self) {
        if let Ok(mut g) = self.0.lock() {
            *g = None;
        }
    }

    /// Upper bound on how long our own holder still keeps the lock; `None` if there is none.
    pub(super) fn remaining(&self) -> Option<Duration> {
        let until = (*self.0.lock().ok()?)?;
        Some(until.saturating_duration_since(Instant::now()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_tracks_begin_and_end() {
        let h = LingeringHold::default();
        assert_eq!(h.remaining(), None);
        h.begin(Duration::from_secs(60));
        let left = h.remaining().unwrap();
        assert!(left > Duration::from_secs(50) && left <= Duration::from_secs(60));
        h.end();
        assert_eq!(h.remaining(), None);
    }
}
