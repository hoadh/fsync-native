//! Desktop notifications. Failures (no notification daemon) are logged, never fatal.

use crate::APP_ID;

/// Show a notification; returns whether it was handed to the daemon.
pub fn show(summary: &str, body: &str) -> bool {
    let result = notify_rust::Notification::new()
        .appname("fsync")
        .icon(APP_ID)
        .summary(summary)
        .body(body)
        .timeout(notify_rust::Timeout::Milliseconds(6000))
        .show();
    match result {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!("notification not shown: {e}");
            false
        }
    }
}
