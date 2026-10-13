//! Encrypted local history (SQLCipher): items, reps, files, search, retention, trust pin.

mod db;
mod list;
mod migrations;
mod new_item;
mod queries;
mod recovery;
mod retention;
mod types;

pub use db::{BlobGuard, DB_FILE, History};
pub use recovery::reset_history;
pub use retention::RetentionReport;
pub use types::{Cursor, Direction, FileRow, FileState, Item, ItemDetail, NewItem, Page, RepRow};
