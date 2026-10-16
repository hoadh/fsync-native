//! fsync engine core: identity, trust, transport, pairing, backend client.

pub mod api;
pub mod blobs;
pub mod clip;
pub mod config;
pub mod engine;
pub mod error;
pub mod events;
pub mod file;
mod fs_util;
pub mod history;
pub mod identity;
pub mod net;
pub mod pair;
pub mod service;
pub mod settings;
pub mod sync;
pub mod trust;

pub use config::EngineConfig;
pub use engine::Engine;
pub use error::{ApiError, CoreError, PairError, Result, TransferError};
pub use events::{EngineEvent, EventSink, OfferRef, PairDecision, PairingEvent, PathState};
pub use fs_util::{ensure_private_dir, write_private_atomic};
pub use identity::Identity;
pub use net::PathTier;
pub use pair::{DeviceInfo, ListBackend, PairSecret};
