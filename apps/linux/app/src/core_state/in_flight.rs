//! Counts operations that may start the engine, so shutdown can close the gate and then wait
//! for the ones that already passed it. The gate check and the count change happen under one
//! lock: an op either sees the closed gate or is counted before shutdown looks.

use super::gate::Gate;
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct InFlight {
    count: Mutex<u32>,
    changed: Condvar,
}

pub struct InFlightGuard<'a>(&'a InFlight);

impl InFlight {
    fn n(&self) -> MutexGuard<'_, u32> {
        self.count.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// `None` once the gate is closed.
    pub fn enter<'a>(&'a self, gate: &Gate) -> Option<InFlightGuard<'a>> {
        let mut n = self.n();
        if !gate.is_open() {
            return None;
        }
        *n += 1;
        Some(InFlightGuard(self))
    }

    /// Waits until nothing is in flight; false if something still is after `d`.
    pub fn wait_idle(&self, d: Duration) -> bool {
        let deadline = Instant::now() + d;
        let mut n = self.n();
        while *n > 0 {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            n = self
                .changed
                .wait_timeout(n, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        true
    }
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        *self.0.n() -= 1;
        self.0.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_gate_refuses_and_shutdown_waits_for_entered_ops() {
        let gate = Gate::default();
        let f = InFlight::default();
        let g = f.enter(&gate).expect("open gate");
        gate.close();
        assert!(f.enter(&gate).is_none());
        assert!(!f.wait_idle(Duration::from_millis(20)));
        drop(g);
        assert!(f.wait_idle(Duration::from_millis(20)));
    }
}
