//! Blocking wait for one core transfer. Core's `fetch_file_to` returns a transfer id at once and
//! reports the end through `on_transfer`; the delegate feeds every event in here and the worker
//! that started the transfer waits for its final one. A final event that arrives before the
//! waiter registers is kept (bounded), so the race cannot lose it.

use fsync_core::service::{TransferDirection, TransferEvent, TransferState};
use std::collections::{HashMap, VecDeque};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

const KEEP: usize = 256;
const SLICE: Duration = Duration::from_millis(100);

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Placed at this path.
    Done(String),
    Failed(String),
    /// Core reported the transfer cancelled.
    Cancelled,
    /// The caller's cancel flag turned true; the transfer is still running in core.
    Aborted,
    TimedOut,
}

#[derive(Default)]
struct Finals {
    done: HashMap<u64, TransferEvent>,
    order: VecDeque<u64>,
}

#[derive(Default)]
pub struct TransferWaiter {
    finals: Mutex<Finals>,
    changed: Condvar,
}

impl TransferWaiter {
    /// Feeds one core event (any direction; only final receive events are kept).
    pub fn on_event(&self, ev: &TransferEvent) {
        let last = matches!(
            ev.state,
            TransferState::Done | TransferState::Failed | TransferState::Cancelled
        );
        if !last || ev.direction != TransferDirection::Receive {
            return;
        }
        let mut f = self.finals.lock().unwrap_or_else(|p| p.into_inner());
        if f.done.insert(ev.transfer_id, ev.clone()).is_none() {
            f.order.push_back(ev.transfer_id);
        }
        while f.order.len() > KEEP {
            if let Some(old) = f.order.pop_front() {
                f.done.remove(&old);
            }
        }
        self.changed.notify_all();
    }

    pub fn wait(&self, id: u64, cancel: &dyn Fn() -> bool, deadline: Instant) -> Outcome {
        let mut f = self.finals.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            if let Some(ev) = f.done.remove(&id) {
                f.order.retain(|x| *x != id);
                return match ev.state {
                    TransferState::Done => match ev.path {
                        Some(p) => Outcome::Done(p),
                        None => Outcome::Failed("the file was not saved".into()),
                    },
                    TransferState::Cancelled => Outcome::Cancelled,
                    _ => Outcome::Failed(ev.message.unwrap_or_else(|| "transfer failed".into())),
                };
            }
            if cancel() {
                return Outcome::Aborted;
            }
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return Outcome::TimedOut;
            };
            f = self
                .changed
                .wait_timeout(f, left.min(SLICE))
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn ev(id: u64, state: TransferState) -> TransferEvent {
        TransferEvent {
            transfer_id: id,
            item_id: "i".into(),
            file_id: Some(0),
            direction: TransferDirection::Receive,
            state,
            bytes_done: 0,
            bytes_total: 0,
            path: Some("/x/y".into()),
            message: Some("boom".into()),
        }
    }

    fn soon() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }

    #[test]
    fn an_early_final_event_is_not_lost() {
        let w = TransferWaiter::default();
        w.on_event(&ev(1, TransferState::Progress));
        w.on_event(&ev(1, TransferState::Done));
        assert_eq!(w.wait(1, &|| false, soon()), Outcome::Done("/x/y".into()));
    }

    #[test]
    fn a_late_event_wakes_the_waiter_and_failures_carry_the_reason() {
        let w = Arc::new(TransferWaiter::default());
        let w2 = Arc::clone(&w);
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            w2.on_event(&ev(2, TransferState::Failed));
        });
        assert_eq!(w.wait(2, &|| false, soon()), Outcome::Failed("boom".into()));
        t.join().expect("join");
    }

    #[test]
    fn cancel_flag_and_deadline_end_the_wait() {
        let w = TransferWaiter::default();
        assert_eq!(w.wait(3, &|| true, soon()), Outcome::Aborted);
        let past = Instant::now();
        assert_eq!(w.wait(3, &|| false, past), Outcome::TimedOut);
        w.on_event(&ev(4, TransferState::Cancelled));
        assert_eq!(w.wait(4, &|| false, soon()), Outcome::Cancelled);
    }

    #[test]
    fn send_events_and_progress_are_ignored_and_memory_is_bounded() {
        let w = TransferWaiter::default();
        let mut send = ev(5, TransferState::Done);
        send.direction = TransferDirection::Send;
        w.on_event(&send);
        assert_eq!(w.wait(5, &|| false, Instant::now()), Outcome::TimedOut);
        for id in 100..100 + 300 {
            w.on_event(&ev(id, TransferState::Done));
        }
        let kept = w.finals.lock().expect("lock").done.len();
        assert_eq!(kept, KEEP);
    }
}
