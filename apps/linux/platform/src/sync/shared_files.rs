//! Inbound file clips: small ones are downloaded into the inbox and put on the clipboard as
//! file references; large ones wait for an explicit download (History).

use super::inbound::ServiceFetcher;
use super::lanes::Cancel;
use super::shared::{Shared, lock};
use super::shared_inbound::pending_info;
use crate::clipboard::formats::files_entry;
use crate::files::EAGER_LIMIT;
use crate::files::inbox::FetchError;
use fsync_core::service::ClipItem;

impl Shared {
    pub(super) fn run_files(&self, item: &ClipItem, cancel: &Cancel) {
        let Some(files) = &self.env.files else {
            return self
                .status
                .event("Received files: they are not supported in this version yet.");
        };
        let info = pending_info(item);
        if item.total_size > EAGER_LIMIT {
            (files.on_large)(item);
            return self.status.event(format!(
                "Large file clip ({} MiB) from {}: open fsync to download.",
                item.total_size.div_ceil(1 << 20),
                info.from_name
            ));
        }
        self.status
            .event(format!("Receiving files from {}...", info.from_name));
        let fetcher = ServiceFetcher(self.service.as_ref());
        match files
            .inbox
            .fetch_item(&fetcher, item, &|| cancel.is_cancelled())
        {
            Ok(paths) => {
                *lock(&self.current_files) = Some(item.item_id.clone());
                self.apply(files_entry(&paths), info);
            }
            Err(FetchError::Cancelled) => {}
            Err(FetchError::Failed(e)) => self
                .status
                .event(format!("Could not receive the files: {e}")),
        }
    }
}
