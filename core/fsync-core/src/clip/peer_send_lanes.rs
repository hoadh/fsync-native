//! Per-peer ordering of clip sends once `publish` no longer waits for slow peers.
//!
//! `publish` returns after a short wait and lets slow peers finish in the background, so the
//! attempt for clip N to a peer may still run while clip N+1 is published. The receiver applies
//! every offer it accepts as the newest clipboard (no ordering check), so N must never reach the
//! peer after N+1. Each peer has one lane: at most one attempt runs (the lane's runner), and
//! behind it waits at most one clip, the newest. A newer clip replaces the waiting one (latest
//! wins); the running attempt keeps its own clip. So a dead peer holds at most two frames and one
//! dial, however many clips are copied while it is away.

use fsync_proto::NodeId;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

struct LaneState<T> {
    /// A runner owns the lane (an attempt is on the wire, or about to be).
    running: bool,
    /// The newest clip queued behind the running attempt.
    waiting: Option<T>,
}

impl<T> Default for LaneState<T> {
    fn default() -> Self {
        Self {
            running: false,
            waiting: None,
        }
    }
}

type Lane<T> = Arc<Mutex<LaneState<T>>>;

fn lock<T>(lane: &Lane<T>) -> std::sync::MutexGuard<'_, LaneState<T>> {
    lane.lock().unwrap_or_else(|p| p.into_inner())
}

/// One lane per paired peer (the map only grows with the paired device list).
pub(crate) struct PeerSendLanes<T>(Mutex<HashMap<NodeId, Lane<T>>>);

impl<T> Default for PeerSendLanes<T> {
    fn default() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

/// What `submit` did with a send.
pub(crate) enum Submit<T> {
    /// The lane was idle: the caller runs `job` now, then asks `runner.next()` for more.
    Run(LaneRunner<T>, T),
    /// An older attempt to this peer still runs: `job` waits behind it. `replaced` is the clip
    /// that waited before, superseded by this one and never sent.
    Wait { replaced: Option<T> },
}

impl<T> PeerSendLanes<T> {
    /// Queues `job` for `peer`. Called in publish order.
    pub(crate) fn submit(&self, peer: NodeId, job: T) -> Submit<T> {
        let lane = self
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(peer)
            .or_default()
            .clone();
        let mut s = lock(&lane);
        if s.running {
            let replaced = s.waiting.replace(job);
            return Submit::Wait { replaced };
        }
        s.running = true;
        drop(s);
        Submit::Run(LaneRunner { lane, done: false }, job)
    }

    /// Frames held by all lanes: running attempts plus waiting clips (for tests).
    #[cfg(test)]
    fn held(&self) -> usize {
        let map = self.0.lock().unwrap();
        map.values()
            .map(|l| {
                let s = lock(l);
                usize::from(s.running) + usize::from(s.waiting.is_some())
            })
            .sum()
    }
}

/// Owns a peer's lane while its attempts run. Dropped without draining (the send task was aborted
/// or never ran, e.g. engine closing; or it panicked): the lane is freed and the waiting clip
/// dropped with it. A dropped clip is not unoffered: its publish sees "no answer" (`failed`),
/// and the offer stays until the outbox expires it, as for every send aborted at close. On a
/// panic (a bug, logged by `BackgroundSends`) that costs one clip that was going to be replaced
/// anyway; a generic lane cannot reach the outbox, and catching panics is not worth it.
pub(crate) struct LaneRunner<T> {
    lane: Lane<T>,
    done: bool,
}

impl<T> LaneRunner<T> {
    /// The clip to send next, after the current attempt ended. `None`: nothing waits, the lane is
    /// idle again and the runner must stop.
    pub(crate) fn next(&mut self) -> Option<T> {
        let mut s = lock(&self.lane);
        let job = s.waiting.take();
        if job.is_none() {
            s.running = false;
            self.done = true;
        }
        job
    }
}

impl<T> Drop for LaneRunner<T> {
    fn drop(&mut self) {
        if !self.done {
            let waiting = {
                let mut s = lock(&self.lane);
                s.running = false;
                s.waiting.take()
            };
            drop(waiting);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(c: char) -> NodeId {
        c.to_string().repeat(64).parse().unwrap()
    }

    fn run<T>(s: Submit<T>) -> (LaneRunner<T>, T) {
        match s {
            Submit::Run(r, j) => (r, j),
            Submit::Wait { .. } => panic!("lane was busy"),
        }
    }

    fn replaced<T>(s: Submit<T>) -> Option<T> {
        match s {
            Submit::Wait { replaced } => replaced,
            Submit::Run(..) => panic!("lane was idle"),
        }
    }

    #[test]
    fn a_newer_clip_replaces_the_waiting_one_the_running_one_keeps_its_own() {
        let lanes = PeerSendLanes::default();
        let (mut r, first) = run(lanes.submit(peer('a'), 1));
        assert_eq!(first, 1);
        assert_eq!(replaced(lanes.submit(peer('a'), 2)), None);
        assert_eq!(
            replaced(lanes.submit(peer('a'), 3)),
            Some(2),
            "2 superseded"
        );
        assert_eq!(r.next(), Some(3));
        assert_eq!(r.next(), None, "lane idle again");
        let (_r, again) = run(lanes.submit(peer('a'), 4));
        assert_eq!(again, 4);
    }

    #[test]
    fn other_peers_have_their_own_lane() {
        let lanes = PeerSendLanes::default();
        let (_a, _) = run(lanes.submit(peer('a'), 1));
        let (_b, _) = run(lanes.submit(peer('b'), 1));
    }

    #[test]
    fn a_burst_behind_a_stuck_attempt_holds_at_most_two_frames() {
        let lanes = PeerSendLanes::default();
        let frame = Arc::new([0u8; 16]);
        let (_r, _running) = run(lanes.submit(peer('a'), frame.clone()));
        for _ in 0..50 {
            drop(replaced(lanes.submit(peer('a'), frame.clone())));
        }
        assert_eq!(lanes.held(), 2);
        assert_eq!(
            Arc::strong_count(&frame),
            3,
            "ours, the running and the waiting one"
        );
    }

    #[test]
    fn an_aborted_runner_frees_the_lane_and_drops_the_waiting_clip() {
        let lanes = PeerSendLanes::default();
        let frame = Arc::new(0u8);
        let (r, running) = run(lanes.submit(peer('a'), frame.clone()));
        assert!(replaced(lanes.submit(peer('a'), frame.clone())).is_none());
        drop((r, running));
        assert_eq!(Arc::strong_count(&frame), 1);
        assert_eq!(lanes.held(), 0);
        let (_r, _) = run(lanes.submit(peer('a'), frame));
    }
}
