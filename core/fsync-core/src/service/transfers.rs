//! Transfer bookkeeping: ids, outgoing push progress (from `FileServed`), incoming fetch tasks.

use super::{
    inner::AbortOnDrop,
    types::{TransferDirection, TransferEvent, TransferState},
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration, Instant},
};
use uuid::Uuid;

const EARLY_CAP: usize = 256;
/// A push nobody pulled is forgotten (and reported failed) after this: the offer itself
/// (outbox ttl 1 h + grace 10 min) is gone by then.
const SEND_TTL: Duration = Duration::from_secs(70 * 60);

struct Send {
    item: Uuid,
    sizes: Vec<u64>,
    served: HashSet<u32>,
    done: u64,
    started: Instant,
}

pub(super) struct Recv {
    pub(super) item: Uuid,
    pub(super) file_id: u32,
    pub(super) total: u64,
    pub(super) _task: AbortOnDrop,
}

#[derive(Default)]
pub(super) struct Transfers {
    next: u64,
    sends: HashMap<u64, Send>,
    pub(super) recvs: HashMap<u64, Recv>,
    /// `FileServed` events that arrived before the send was registered.
    early: VecDeque<(Uuid, u32)>,
}

pub(super) fn event(
    id: u64,
    item: Uuid,
    dir: TransferDirection,
    state: TransferState,
) -> TransferEvent {
    TransferEvent {
        transfer_id: id,
        item_id: item.to_string(),
        file_id: None,
        direction: dir,
        state,
        bytes_done: 0,
        bytes_total: 0,
        path: None,
        message: None,
    }
}

impl Transfers {
    pub(super) fn next_id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    pub(super) fn abort_all(&mut self) {
        self.recvs.clear();
        self.sends.clear();
    }

    /// Starts tracking a push; returns its id and the events to emit (Started, plus whatever
    /// the receiver already pulled).
    pub(super) fn register_send(
        &mut self,
        item: Uuid,
        sizes: Vec<u64>,
    ) -> (u64, Vec<TransferEvent>) {
        self.register_send_at(item, sizes, Instant::now())
    }

    fn register_send_at(
        &mut self,
        item: Uuid,
        sizes: Vec<u64>,
        now: Instant,
    ) -> (u64, Vec<TransferEvent>) {
        let mut out = self.expire_sends(now);
        let id = self.next_id();
        let total = sizes.iter().sum();
        let mut start = event(id, item, TransferDirection::Send, TransferState::Started);
        start.bytes_total = total;
        out.push(start);
        self.sends.insert(
            id,
            Send {
                item,
                sizes,
                served: HashSet::new(),
                done: 0,
                started: now,
            },
        );
        let early: Vec<_> = self
            .early
            .iter()
            .copied()
            .filter(|(i, _)| *i == item)
            .collect();
        self.early.retain(|(i, _)| *i != item);
        for (i, f) in early {
            out.extend(self.on_served(i, f));
        }
        (id, out)
    }

    /// Drops pushes older than `SEND_TTL`; returns their `Failed` events.
    fn expire_sends(&mut self, now: Instant) -> Vec<TransferEvent> {
        let stale: Vec<u64> = self
            .sends
            .iter()
            .filter(|(_, s)| now.saturating_duration_since(s.started) >= SEND_TTL)
            .map(|(id, _)| *id)
            .collect();
        stale
            .into_iter()
            .filter_map(|id| self.sends.remove(&id).map(|s| (id, s)))
            .map(|(id, s)| {
                let mut e = event(id, s.item, TransferDirection::Send, TransferState::Failed);
                e.message = Some("receiver never pulled the files".into());
                e
            })
            .collect()
    }

    /// A receiver finished pulling `file_id` of `item`. Unknown items are remembered briefly.
    pub(super) fn on_served(&mut self, item: Uuid, file_id: u32) -> Vec<TransferEvent> {
        let mut out = self.expire_sends(Instant::now());
        out.extend(self.serve(item, file_id));
        out
    }

    fn serve(&mut self, item: Uuid, file_id: u32) -> Vec<TransferEvent> {
        let Some((&id, s)) = self.sends.iter_mut().find(|(_, s)| s.item == item) else {
            if self.early.len() >= EARLY_CAP {
                self.early.pop_front();
            }
            self.early.push_back((item, file_id));
            return vec![];
        };
        let Some(&size) = s.sizes.get(file_id as usize) else {
            return vec![];
        };
        if !s.served.insert(file_id) {
            return vec![];
        }
        s.done += size;
        let total: u64 = s.sizes.iter().sum();
        let mut p = event(id, item, TransferDirection::Send, TransferState::Progress);
        (p.file_id, p.bytes_done, p.bytes_total) = (Some(file_id), s.done, total);
        let mut out = vec![p];
        if s.served.len() == s.sizes.len() {
            let mut d = event(id, item, TransferDirection::Send, TransferState::Done);
            (d.bytes_done, d.bytes_total) = (total, total);
            out.push(d);
            self.sends.remove(&id);
        }
        out
    }

    /// Stops tracking `id`; returns the Cancelled event if it was running.
    pub(super) fn cancel(&mut self, id: u64) -> Option<TransferEvent> {
        if let Some(s) = self.sends.remove(&id) {
            return Some(event(
                id,
                s.item,
                TransferDirection::Send,
                TransferState::Cancelled,
            ));
        }
        let r = self.recvs.remove(&id)?;
        let mut e = event(
            id,
            r.item,
            TransferDirection::Receive,
            TransferState::Cancelled,
        );
        (e.file_id, e.bytes_total) = (Some(r.file_id), r.total);
        Some(e)
    }
}

#[cfg(test)]
mod tests;
