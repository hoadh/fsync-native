//! Cap on presence sockets that have not authenticated yet (global and per client IP).

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub struct UnauthGate {
    global: Arc<Semaphore>,
    per_ip: Arc<Mutex<HashMap<String, usize>>>,
    per_ip_max: usize,
}

/// Held while a socket is unauthenticated; dropping it frees both slots.
pub struct UnauthPermit {
    _global: OwnedSemaphorePermit,
    per_ip: Arc<Mutex<HashMap<String, usize>>>,
    ip: String,
}

impl UnauthGate {
    pub fn new(global_max: usize, per_ip_max: usize) -> Self {
        Self {
            global: Arc::new(Semaphore::new(global_max)),
            per_ip: Arc::default(),
            per_ip_max,
        }
    }

    /// `None` when either cap is reached.
    pub fn try_acquire(&self, ip: &str) -> Option<UnauthPermit> {
        let mut m = self.per_ip.lock().unwrap_or_else(|e| e.into_inner());
        if m.get(ip).copied().unwrap_or(0) >= self.per_ip_max {
            return None;
        }
        let global = self.global.clone().try_acquire_owned().ok()?;
        *m.entry(ip.to_owned()).or_insert(0) += 1;
        Some(UnauthPermit {
            _global: global,
            per_ip: self.per_ip.clone(),
            ip: ip.to_owned(),
        })
    }
}

impl Drop for UnauthPermit {
    fn drop(&mut self) {
        let mut m = self.per_ip.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = m.get_mut(&self.ip) {
            *n -= 1;
            if *n == 0 {
                m.remove(&self.ip);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_per_ip_and_globally_and_releases_on_drop() {
        let g = UnauthGate::new(3, 2);
        let a1 = g.try_acquire("a").unwrap();
        let _a2 = g.try_acquire("a").unwrap();
        assert!(g.try_acquire("a").is_none(), "per-ip cap");
        let _b = g.try_acquire("b").unwrap();
        assert!(g.try_acquire("c").is_none(), "global cap");
        drop(a1);
        assert!(g.try_acquire("c").is_some());
    }
}
