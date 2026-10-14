//! Phase-03 spike for C# (uniffi-bindgen-cs): callback stress and buffer copy cost.
use std::sync::Arc;

uniffi::setup_scaffolding!();

#[uniffi::export(with_foreign)]
pub trait Sink: Send + Sync {
    fn on_event(&self, seq: u64, payload: Vec<u8>);
}

/// Calls `sink.on_event` `count` times from a fresh Rust thread (like core's
/// "fsync-callbacks" thread) and returns once all calls finished.
#[uniffi::export]
pub fn stress_callbacks(sink: Arc<dyn Sink>, count: u64, payload_len: u32) {
    let t = std::thread::Builder::new()
        .name("spike-callbacks".into())
        .spawn(move || {
            let payload = vec![0xA5u8; payload_len as usize];
            for seq in 0..count {
                sink.on_event(seq, payload.clone());
            }
        })
        .expect("spawn");
    t.join().expect("callback thread panicked");
}

/// Same bytes back: one copy in, one copy out.
#[uniffi::export]
pub fn echo(data: Vec<u8>) -> Vec<u8> {
    data
}
