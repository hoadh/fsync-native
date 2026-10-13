//! `TrustStore` on the `trust` table of the history database (single row: the pinned head).
//!
//! Tied to the history key: if that key is lost the table goes with the database and the
//! device has to pair again. Use `FileTrustStore` where trust must outlive the keystore.

use super::{PinnedState, TrustStore};
use crate::{
    error::{CoreError, Result},
    history::History,
};
use fsync_proto::SignedDeviceList;
use rusqlite::{Connection, OptionalExtension, params};
use std::sync::{Arc, Mutex};

pub struct SqliteTrustStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteTrustStore {
    /// Shares the history writer connection (writes are serialized with history writes).
    pub fn new(history: &History) -> Self {
        Self {
            conn: history.0.w.clone(),
        }
    }

    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut g = self
            .conn
            .lock()
            .map_err(|_| CoreError::Store("history lock poisoned".into()))?;
        f(&mut g)
    }
}

impl TrustStore for SqliteTrustStore {
    fn load(&self) -> Result<Option<PinnedState>> {
        let row: Option<(Vec<u8>, Vec<u8>)> = self.with(|c| {
            Ok(c.query_row(
                "SELECT hash, signed_bytes FROM trust ORDER BY seq DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
        })?;
        row.map(|(hash, bytes)| {
            Ok(PinnedState {
                root_hash: hash
                    .try_into()
                    .map_err(|_| CoreError::Store("corrupt trust row".into()))?,
                head: SignedDeviceList::from_bytes(&bytes)?,
            })
        })
        .transpose()
    }

    fn save(&self, state: &PinnedState) -> Result<()> {
        let seq = i64::try_from(state.head.decode()?.seq)
            .map_err(|_| CoreError::Invalid("seq out of range"))?;
        let bytes = state.head.to_bytes()?;
        self.with(|c| {
            let tx = c.transaction()?;
            tx.execute("DELETE FROM trust", [])?;
            tx.execute(
                "INSERT INTO trust(seq, hash, signed_bytes) VALUES (?1, ?2, ?3)",
                params![seq, state.root_hash.as_slice(), bytes],
            )?;
            Ok(tx.commit()?)
        })
    }

    fn clear(&self) -> Result<()> {
        self.with(|c| Ok(c.execute("DELETE FROM trust", []).map(|_| ())?))
    }
}
