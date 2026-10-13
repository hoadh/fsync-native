//! Single-flight per blob hash: concurrent fetches of the same content must not share a `.part`.

use crate::blobs::Hash;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

#[derive(Default)]
pub struct FetchLocks(Mutex<HashMap<Hash, Weak<AsyncMutex<()>>>>);

impl FetchLocks {
    /// Wait for exclusive right to fetch `hash`; released on drop.
    pub async fn lock(&self, hash: &Hash) -> OwnedMutexGuard<()> {
        let m = {
            let mut g = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            g.retain(|_, w| w.strong_count() > 0);
            match g.get(hash).and_then(Weak::upgrade) {
                Some(m) => m,
                None => {
                    let m = Arc::new(AsyncMutex::new(()));
                    g.insert(*hash, Arc::downgrade(&m));
                    m
                }
            }
        };
        m.lock_owned().await
    }
}
