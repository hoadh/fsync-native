//! file/1: pull-based, blake3-verified, resumable transfer of reps and files.
//!
//! Backend choice (ADR 007, addendum phase 07): own protocol over plain QUIC bi-streams with a
//! flattened chunk-group outboard (`group_tree`). All of it sits behind `FetchSession` /
//! `fetch_to_store`, so swapping in iroh-blobs later only touches this module.

mod fetch_client;
mod fetch_server;
mod group_tree;
mod locks;
mod outbox;
mod policy;
mod range_reader;
mod reader_cache;
mod resume;
mod served;

pub use fetch_client::Expect;
pub use group_tree::{Cv, GROUP, Verifier, digest_read, group_count, root_of};
pub use policy::{
    MAX_CV_BYTES, check_declared_size, check_fetch, fetch_deadline, max_verifiable_size,
};
pub use resume::{FetchStats, fetch_to_store};
pub use served::{Served, Source};

pub(crate) use fetch_server::FileHandler;
pub(crate) use locks::FetchLocks;

pub(crate) use outbox::{Entry, Outbox};
pub(crate) use range_reader::RangeReader;
pub use reader_cache::{MAX_READERS, ReaderCache};
