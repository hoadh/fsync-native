//! Building a `NewItem` from a clip offer (sent or received).

use super::types::{Direction, FileRow, FileState, NewItem, RepRow};
use fsync_proto::{ClipOffer, NodeId};

impl NewItem {
    /// `sensitive` items are never stored. For received offers only inline reps are present
    /// until fetched; sent items are persisted by the sender, so all reps are present.
    pub fn from_offer(
        offer: &ClipOffer,
        direction: Direction,
        peer: Option<NodeId>,
        sensitive: bool,
    ) -> Self {
        let reps = offer
            .reps
            .iter()
            .zip(&offer.inline)
            .enumerate()
            .map(|(i, (r, inline))| RepRow {
                idx: i as u32,
                mime: r.mime.clone(),
                size: r.size,
                hash: r.hash,
                present: direction == Direction::Out || inline.is_some(),
            })
            .collect();
        let files = offer
            .files
            .iter()
            .map(|f| FileRow {
                file_id: f.file_id,
                name: f.name.clone(),
                rel_path: f.rel_path.clone(),
                size: f.size,
                hash: f.hash,
                local_path: None,
                state: FileState::Offered,
            })
            .collect();
        Self {
            id: offer.item_id,
            direction,
            peer,
            kind: offer.kind,
            created_ms: offer.ts_ms,
            preview: offer.preview.clone(),
            sensitive,
            reps,
            files,
        }
    }
}
