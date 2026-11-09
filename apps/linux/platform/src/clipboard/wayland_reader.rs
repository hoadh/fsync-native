//! Reads one data-control offer from any thread (the sync engine's lane), never on the
//! Wayland dispatch thread.

use super::wayland_proto::Offer;
use super::wayland_source::{make_pipe, read_pipe};
use super::{ChangeReader, ReadError};
use std::os::fd::AsFd;
use std::sync::Mutex;
use std::time::Duration;
use wayland_client::Connection;

/// Reads one offer's bytes from any thread; `None` once a newer selection replaced it.
pub(super) struct OfferReader {
    pub(super) offer: Mutex<Option<Offer>>,
    pub(super) conn: Connection,
}

impl OfferReader {
    pub(super) fn retire(&self) {
        if let Some(o) = self.offer.lock().unwrap_or_else(|p| p.into_inner()).take() {
            o.destroy();
        }
    }
}

impl ChangeReader for OfferReader {
    fn read(&self, mime: &str, cap: usize, timeout: Duration) -> Result<Vec<u8>, ReadError> {
        let (r, w) = make_pipe().map_err(|e| ReadError::Io(e.kind().to_string()))?;
        {
            let guard = self.offer.lock().unwrap_or_else(|p| p.into_inner());
            guard
                .as_ref()
                .ok_or(ReadError::Stale)?
                .receive(mime, w.as_fd());
        }
        drop(w); // EOF must arrive when the source closes its end
        self.conn
            .flush()
            .map_err(|_| ReadError::Io("flush".into()))?;
        read_pipe(r, cap, timeout)
    }
}
