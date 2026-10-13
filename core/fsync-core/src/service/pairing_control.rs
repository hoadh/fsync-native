//! Pairing control calls: answer the SAS prompt, cancel.

use super::{
    error::{FsyncError, SvcResult},
    inner::{Inner, lock},
};

impl Inner {
    /// Cancels whatever pairing is in progress: closes the offer window, aborts the task,
    /// rejects a pending SAS confirmation and wakes a waiting `pairing_join`.
    pub(super) fn pairing_cancel(&self) -> SvcResult<()> {
        let slot = std::mem::take(&mut *lock(&self.pairing)?);
        drop(slot.task);
        if let Some((_, d)) = slot.decision {
            d.reject();
        }
        if let Some(w) = slot.join_waiter {
            let _ = w.send(Err(FsyncError::Cancelled));
        }
        if let Ok(e) = self.engine() {
            e.node().close_pairing();
        }
        Ok(())
    }

    pub(super) fn pairing_confirm(&self, request_id: u64, accept: bool) -> SvcResult<()> {
        let mut slot = lock(&self.pairing)?;
        match slot.decision.as_ref() {
            None => return Err(FsyncError::invalid("no pairing waiting for confirmation")),
            // An answer to a request that has been replaced must not decide the new one.
            Some((id, _)) if *id != request_id => {
                return Err(FsyncError::invalid("stale pairing confirmation"));
            }
            Some(_) => {}
        }
        if let Some((_, d)) = slot.decision.take() {
            if accept { d.accept() } else { d.reject() }
        }
        Ok(())
    }
}
