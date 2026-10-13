//! clip/1: sender announces a clipboard item, receivers pull what they need.

mod echo_guard;
mod input;
mod limits;
mod offer;
mod rate;
mod receive;
mod received_log;
mod send;

pub(crate) use echo_guard::EchoGuard;
pub use echo_guard::set_hash;
pub use input::{ClipInput, FileInput, PublishOpts, PublishOutcome, Published, RepInput};
pub use offer::now_ms;
pub(crate) use rate::PeerRate;
pub(crate) use receive::ClipHandler;
pub use received_log::{KEEP_BYTES, ReceivedLog, ReceivedOffer, checked_total};
