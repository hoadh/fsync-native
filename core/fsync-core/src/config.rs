//! Engine tunables (clip/file protocols). Defaults follow the phase-07 spec.

#[cfg(any(test, feature = "test-hooks"))]
use std::sync::Arc;
use std::time::Duration;

/// Test hook (feature `test-hooks`): may mutate bytes the sender is about to put on the wire (offset, data).
#[cfg(any(test, feature = "test-hooks"))]
#[doc(hidden)]
pub type ServeHook = Arc<dyn Fn(u64, &mut [u8]) + Send + Sync>;

#[derive(Clone)]
pub struct EngineConfig {
    /// Largest clipboard item (sum of reps) we send or accept.
    pub max_clip_bytes: u64,
    /// Largest single file we agree to pull (`None`: only free disk space and the verifier's
    /// own bound limit it).
    pub max_file_bytes: Option<u64>,
    /// Outbox entry lifetime.
    pub outbox_ttl: Duration,
    /// Extra lifetime of an entry once a newer clip was published.
    pub outbox_grace: Duration,
    /// Reps above this size are spilled from memory to the blob store.
    pub spill_threshold: usize,
    /// Per-peer deadline for delivering an offer.
    pub send_timeout: Duration,
    /// Deadline for one `read_file_range`.
    pub read_timeout: Duration,
    #[cfg(any(test, feature = "test-hooks"))]
    #[doc(hidden)]
    pub serve_hook: Option<ServeHook>,
    /// Test hook (feature `test-hooks`): the sender waits this long before answering each file/1
    /// request, which stands in for one network round trip.
    #[cfg(any(test, feature = "test-hooks"))]
    #[doc(hidden)]
    pub serve_delay: Option<Duration>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_clip_bytes: 50 * 1024 * 1024,
            max_file_bytes: None,
            outbox_ttl: Duration::from_secs(3600),
            outbox_grace: Duration::from_secs(600),
            spill_threshold: 1024 * 1024,
            send_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(30),
            #[cfg(any(test, feature = "test-hooks"))]
            serve_hook: None,
            #[cfg(any(test, feature = "test-hooks"))]
            serve_delay: None,
        }
    }
}
