//! One lane = one worker thread with a single "newest wins" slot (ADR 013 decision 11). A job
//! submitted while another is queued replaces it, and the running job sees `is_cancelled()`
//! turn true so it can drop a result nobody wants any more. Panics in a job are contained and
//! logged, never silent.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Handed to a running job: true once a newer job was submitted (or the lane stopped).
#[derive(Clone)]
pub struct Cancel {
    generation: Arc<AtomicU64>,
    mine: u64,
}

impl Cancel {
    pub fn is_cancelled(&self) -> bool {
        self.generation.load(Ordering::SeqCst) != self.mine
    }
}

struct State<T> {
    slot: Option<T>,
    stop: bool,
    busy: bool,
}

/// Cheap clonable submit side of a lane (for other threads that feed it).
pub struct LaneHandle<T: Send + 'static> {
    state: Arc<(Mutex<State<T>>, Condvar)>,
    generation: Arc<AtomicU64>,
}

impl<T: Send + 'static> Clone for LaneHandle<T> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
            generation: Arc::clone(&self.generation),
        }
    }
}

impl<T: Send + 'static> LaneHandle<T> {
    /// Queues `item`, replacing a queued one and cancelling the running one.
    pub fn submit(&self, item: T) {
        let (lock, cv) = &*self.state;
        let mut s = lock.lock().unwrap_or_else(|p| p.into_inner());
        if s.stop {
            return;
        }
        self.generation.fetch_add(1, Ordering::SeqCst);
        s.slot = Some(item);
        cv.notify_all();
    }
}

pub struct Lane<T: Send + 'static> {
    handle: LaneHandle<T>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl<T: Send + 'static> Lane<T> {
    pub fn spawn(name: &str, job: impl Fn(T, Cancel) + Send + 'static) -> std::io::Result<Self> {
        let state = Arc::new((
            Mutex::new(State {
                slot: None,
                stop: false,
                busy: false,
            }),
            Condvar::new(),
        ));
        let generation = Arc::new(AtomicU64::new(0));
        let (st, gen_) = (Arc::clone(&state), Arc::clone(&generation));
        let label = name.to_owned();
        let handle = std::thread::Builder::new()
            .name(format!("fsync-lane-{name}"))
            .spawn(move || {
                let (lock, cv) = &*st;
                loop {
                    let (item, mine) = {
                        let mut s = lock.lock().unwrap_or_else(|p| p.into_inner());
                        loop {
                            if s.stop {
                                return;
                            }
                            if let Some(item) = s.slot.take() {
                                s.busy = true;
                                break (item, gen_.load(Ordering::SeqCst));
                            }
                            s = cv.wait(s).unwrap_or_else(|p| p.into_inner());
                        }
                    };
                    let cancel = Cancel {
                        generation: Arc::clone(&gen_),
                        mine,
                    };
                    if catch_unwind(AssertUnwindSafe(|| job(item, cancel))).is_err() {
                        tracing::error!("clipboard lane '{label}': a job panicked");
                    }
                    lock.lock().unwrap_or_else(|p| p.into_inner()).busy = false;
                    cv.notify_all();
                }
            })?;
        Ok(Self {
            handle: LaneHandle { state, generation },
            thread: Mutex::new(Some(handle)),
        })
    }

    pub fn submit(&self, item: T) {
        self.handle.submit(item);
    }

    pub fn handle(&self) -> LaneHandle<T> {
        self.handle.clone()
    }

    /// Cancels the running job, drops the queued one and stops the thread; waits up to
    /// `timeout` for the running job to end (a job stuck in the network is abandoned).
    pub fn shutdown(&self, timeout: Duration) -> bool {
        let (lock, cv) = &*self.handle.state;
        self.handle.generation.fetch_add(1, Ordering::SeqCst);
        let deadline = Instant::now() + timeout;
        let mut s = lock.lock().unwrap_or_else(|p| p.into_inner());
        s.stop = true;
        s.slot = None;
        cv.notify_all();
        while s.busy {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            s = cv
                .wait_timeout(s, left)
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        drop(s);
        if let Some(t) = self.thread.lock().unwrap_or_else(|p| p.into_inner()).take() {
            let _ = t.join();
        }
        true
    }
}

#[cfg(test)]
#[path = "lanes_tests.rs"]
mod tests;
