//! Lifecycle gate (closed on shutdown: no new ops, no IPC work) and per-operation generation ids
//! (a result is applied only if no newer op of the same kind started meanwhile).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Clone, Copy, Debug)]
pub enum OpKind {
    Login,
    Pairing,
    Devices,
    Account,
    History,
}

#[derive(Default)]
pub struct Gate {
    closed: AtomicBool,
    gens: [AtomicU64; 5],
}

impl Gate {
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    pub fn is_open(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
    }

    /// Starts a new op of `kind`; older ones become stale.
    pub fn begin(&self, kind: OpKind) -> u64 {
        self.gens[kind as usize].fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Generation of the newest op of `kind` (without starting one).
    pub fn current(&self, kind: OpKind) -> u64 {
        self.gens[kind as usize].load(Ordering::SeqCst)
    }

    pub fn is_current(&self, kind: OpKind, generation: u64) -> bool {
        self.gens[kind as usize].load(Ordering::SeqCst) == generation
    }

    /// Makes every op of `kind` in flight stale (cancel).
    pub fn invalidate(&self, kind: OpKind) {
        self.gens[kind as usize].fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_op_makes_older_stale() {
        let g = Gate::default();
        let a = g.begin(OpKind::Pairing);
        assert!(g.is_current(OpKind::Pairing, a));
        let b = g.begin(OpKind::Pairing);
        assert!(!g.is_current(OpKind::Pairing, a) && g.is_current(OpKind::Pairing, b));
        assert!(g.is_current(OpKind::Login, g.begin(OpKind::Login)));
    }

    #[test]
    fn close_is_sticky_and_invalidate_stales() {
        let g = Gate::default();
        let a = g.begin(OpKind::Devices);
        g.invalidate(OpKind::Devices);
        assert!(!g.is_current(OpKind::Devices, a));
        assert!(g.is_open());
        g.close();
        assert!(!g.is_open());
    }
}
