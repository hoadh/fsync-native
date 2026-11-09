//! Single-flight guard and wake-up for the boot thread: "Try again" may re-run boot only when no
//! boot is running, the 10 s keyring poll can be cut short (Unlock button, shutdown), and
//! shutdown can wait briefly for an in-flight boot before it reads the service.

use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[derive(Default)]
struct State {
    running: bool,
    woken: bool,
}

#[derive(Default)]
pub struct BootControl {
    state: Mutex<State>,
    changed: Condvar,
}

impl BootControl {
    fn st(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// True if the caller is now the one boot (it must call `finish`); false while one runs.
    pub fn try_begin(&self) -> bool {
        let mut st = self.st();
        if st.running {
            return false;
        }
        st.running = true;
        st.woken = false;
        true
    }

    /// The boot ended (success, failure or shutdown): the next one may start.
    pub fn finish(&self) {
        self.st().running = false;
        self.changed.notify_all();
    }

    /// Cuts the current `sleep` short (user pressed Unlock, shutdown began).
    pub fn wake(&self) {
        self.st().woken = true;
        self.changed.notify_all();
    }

    /// Sleeps up to `d` or until `wake`.
    pub fn sleep(&self, d: Duration) {
        let deadline = Instant::now() + d;
        let mut st = self.st();
        while !st.woken {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            st = self
                .changed
                .wait_timeout(st, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        st.woken = false;
    }

    /// Waits until no boot runs; false if it still runs after `d`.
    pub fn wait_idle(&self, d: Duration) -> bool {
        let deadline = Instant::now() + d;
        let mut st = self.st();
        while st.running {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            st = self
                .changed
                .wait_timeout(st, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn only_one_boot_runs_and_idle_wait_sees_it_end() {
        let c = Arc::new(BootControl::default());
        assert!(c.try_begin(), "first boot");
        assert!(!c.try_begin(), "single flight");
        assert!(!c.wait_idle(Duration::from_millis(20)));
        let c2 = Arc::clone(&c);
        let t = std::thread::spawn(move || c2.wait_idle(Duration::from_secs(2)));
        c.finish();
        assert!(t.join().unwrap());
        assert!(c.try_begin(), "a new boot may start after the last ended");
    }

    #[test]
    fn wake_cuts_the_sleep_short() {
        let c = Arc::new(BootControl::default());
        let c2 = Arc::clone(&c);
        let t = std::thread::spawn(move || {
            let start = Instant::now();
            c2.sleep(Duration::from_secs(30));
            start.elapsed()
        });
        std::thread::sleep(Duration::from_millis(50));
        c.wake();
        assert!(t.join().unwrap() < Duration::from_secs(5));
    }
}
