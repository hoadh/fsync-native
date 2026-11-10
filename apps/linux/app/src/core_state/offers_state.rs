//! Offer rows of pushed files and the rule that keeps their list bounded.

use fsync_core::service::{ClipItem, FileInfo};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Most offers kept (finished ones are dropped oldest first).
pub const MAX_OFFERS: usize = 50;
/// Offers waiting for an answer or being received; further pushes are declined (no thread,
/// no notification) so a chatty peer cannot flood the UI.
pub const MAX_PENDING: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfferState {
    Pending,
    Receiving,
    Done,
    Failed,
    Declined,
    Cancelled,
}

impl OfferState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Receiving => "receiving",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Declined => "declined",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn finished(self) -> bool {
        !matches!(self, Self::Pending | Self::Receiving)
    }
}

/// One pushed-files offer ("Send to this device" from another device).
#[derive(Clone, Debug)]
pub struct OfferRow {
    pub item_id: String,
    pub from_name: String,
    pub files: Vec<FileInfo>,
    pub total: u64,
    pub state: OfferState,
    /// Bytes of the files that are complete.
    pub finished_bytes: u64,
    /// Bytes of the file in flight.
    pub active_bytes: u64,
    /// Core transfer id of the file in flight (to cancel it).
    pub active_transfer: Option<u64>,
    /// Failure sentence, or where the files went (`Done`).
    pub message: String,
    pub dir: String,
    /// Set by Cancel; the accept loop and the transfer wait watch it.
    pub cancel: Arc<AtomicBool>,
}

impl OfferRow {
    pub fn new(offer: &ClipItem) -> Self {
        Self {
            item_id: offer.item_id.clone(),
            from_name: if offer.from_name.is_empty() {
                "Another device".into()
            } else {
                offer.from_name.clone()
            },
            files: offer.files.clone(),
            total: offer.total_size,
            state: OfferState::Pending,
            finished_bytes: 0,
            active_bytes: 0,
            active_transfer: None,
            message: String::new(),
            dir: String::new(),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn done_bytes(&self) -> u64 {
        self.finished_bytes
            .saturating_add(self.active_bytes)
            .min(self.total)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Admit {
    Added,
    Duplicate,
    /// Too many offers waiting: a finished "declined" row with a note was added instead.
    Declined,
}

/// Books `row` in `offers`, keeping the list bounded: at most `MAX_PENDING` active offers and
/// `MAX_OFFERS` rows (finished ones go first, oldest first).
pub fn admit_offer(offers: &mut Vec<OfferRow>, mut row: OfferRow) -> Admit {
    if offers.iter().any(|o| o.item_id == row.item_id) {
        return Admit::Duplicate;
    }
    let active = offers.iter().filter(|o| !o.state.finished()).count();
    let verdict = if active >= MAX_PENDING {
        row.state = OfferState::Declined;
        row.message = "Too many offers are waiting; this one was declined.".into();
        Admit::Declined
    } else {
        Admit::Added
    };
    offers.push(row);
    while offers.len() > MAX_OFFERS {
        match offers.iter().position(|o| o.state.finished()) {
            Some(i) => {
                offers.remove(i);
            }
            None => break,
        }
    }
    verdict
}

#[cfg(test)]
mod tests {
    use super::*;
    use fsync_proto::ClipKind;

    fn item(id: &str) -> ClipItem {
        ClipItem {
            item_id: id.into(),
            from: "n".into(),
            from_name: String::new(),
            kind: ClipKind::Files,
            ts_ms: 0,
            preview: None,
            reps: vec![],
            files: vec![],
            total_size: 10,
            push: true,
        }
    }

    #[test]
    fn offers_are_capped_deduplicated_and_trimmed() {
        let mut v = Vec::new();
        for i in 0..MAX_PENDING {
            assert_eq!(
                admit_offer(&mut v, OfferRow::new(&item(&format!("{i:x}")))),
                Admit::Added
            );
        }
        assert_eq!(
            admit_offer(&mut v, OfferRow::new(&item("0"))),
            Admit::Duplicate
        );
        assert_eq!(
            admit_offer(&mut v, OfferRow::new(&item("ff"))),
            Admit::Declined
        );
        let extra = v.last().expect("row");
        assert_eq!(extra.state, OfferState::Declined);
        assert!(!extra.message.is_empty());
        // Finished rows make room: the list never exceeds MAX_OFFERS.
        for i in 0..(MAX_OFFERS * 2) {
            admit_offer(&mut v, OfferRow::new(&item(&format!("a{i:x}"))));
        }
        assert!(v.len() <= MAX_OFFERS);
        assert!(v.iter().filter(|o| !o.state.finished()).count() <= MAX_PENDING);
    }
}
