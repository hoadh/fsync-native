//! Keeps `Hello` messages under the server's per-minute limit when addresses flap.

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

const WINDOW: Duration = Duration::from_secs(60);
/// Server allows 10 per minute; stay below it.
const MAX_PER_WINDOW: usize = 8;

#[derive(Default)]
pub(super) struct HelloPacer(VecDeque<Instant>);

impl HelloPacer {
    pub(super) fn record(&mut self, now: Instant) {
        self.0.push_back(now);
    }

    /// Earliest instant a `Hello` may go out.
    pub(super) fn next_slot(&mut self, now: Instant) -> Instant {
        while self
            .0
            .front()
            .is_some_and(|t| now.duration_since(*t) >= WINDOW)
        {
            self.0.pop_front();
        }
        if self.0.len() < MAX_PER_WINDOW {
            return now;
        }
        self.0.front().map_or(now, |t| *t + WINDOW)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_burst_then_defers_until_oldest_expires() {
        let t0 = Instant::now();
        let mut p = HelloPacer::default();
        for _ in 0..MAX_PER_WINDOW {
            assert_eq!(p.next_slot(t0), t0);
            p.record(t0);
        }
        assert_eq!(p.next_slot(t0), t0 + WINDOW);
        let later = t0 + WINDOW;
        assert_eq!(p.next_slot(later), later);
    }
}
