//! User-facing text for core errors. Exhaustive over `FsyncError` (a new variant must be
//! phrased here), never the raw `Debug` form.

use fsync_core::service::FsyncError;

pub const ALREADY_RUNNING: &str =
    "fsync is already running for this data directory. Use the open window or the tray icon.";

pub fn for_error(e: &FsyncError) -> String {
    match e {
        FsyncError::NotPaired => "That device is not paired.".into(),
        FsyncError::PeerOffline => "That device is offline.".into(),
        FsyncError::Network(m) => format!("Can't reach the fsync server ({m})."),
        FsyncError::Integrity => "The data failed verification and was discarded.".into(),
        FsyncError::TooLarge => "That is larger than the size limit.".into(),
        FsyncError::Auth => "Please sign in again.".into(),
        FsyncError::ReauthRequired => "For this, sign in again first.".into(),
        FsyncError::Io(m) => format!("File error: {m}"),
        FsyncError::Cancelled => "Cancelled.".into(),
        FsyncError::HistoryKeyMismatch => {
            "The clipboard history was encrypted with a different key.".into()
        }
        FsyncError::NotFound => "It no longer exists.".into(),
        FsyncError::Invalid(m) => m.clone(),
        FsyncError::EngineAlreadyRunning => ALREADY_RUNNING.into(),
        FsyncError::Internal(m) => format!("Something went wrong ({m})."),
        FsyncError::PeerRefused => {
            "That device removed this computer. Pair again to send to it.".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_plain_sentences_not_debug_output() {
        let all = [
            FsyncError::NotPaired,
            FsyncError::PeerOffline,
            FsyncError::Network("timeout".into()),
            FsyncError::Integrity,
            FsyncError::TooLarge,
            FsyncError::Auth,
            FsyncError::ReauthRequired,
            FsyncError::Io("disk".into()),
            FsyncError::Cancelled,
            FsyncError::HistoryKeyMismatch,
            FsyncError::NotFound,
            FsyncError::Invalid("bad code".into()),
            FsyncError::EngineAlreadyRunning,
            FsyncError::Internal("x".into()),
            FsyncError::PeerRefused,
        ];
        for e in all {
            let m = for_error(&e);
            assert!(
                !m.is_empty() && !m.contains("::") && !m.contains('{'),
                "{m}"
            );
        }
        assert!(for_error(&FsyncError::PeerRefused).contains("removed"));
        assert_eq!(
            for_error(&FsyncError::Invalid("bad code".into())),
            "bad code"
        );
    }
}
