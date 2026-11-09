//! When the keyring may be asked to show an unlock prompt.

use std::sync::atomic::{AtomicBool, Ordering};

/// Who may make the keyring show an unlock prompt. Retrying every 10 s while the keyring is
/// locked must not stack prompts, so automatic retries prompt once per launch; after that only
/// an explicit user action (`request`) allows one more prompt.
#[derive(Default)]
pub struct UnlockPolicy {
    auto_used: AtomicBool,
    user_asked: AtomicBool,
}

impl UnlockPolicy {
    /// The "Unlock keyring" button: the next locked access may prompt once.
    pub fn request(&self) {
        self.user_asked.store(true, Ordering::SeqCst);
    }

    /// Consumes one prompt permission, if any is left.
    pub(super) fn may_prompt(&self) -> bool {
        let user = self.user_asked.swap(false, Ordering::SeqCst);
        let first = !self.auto_used.swap(true, Ordering::SeqCst);
        user || first
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_retries_prompt_once_then_only_on_user_request() {
        let p = UnlockPolicy::default();
        assert!(p.may_prompt(), "first locked access may prompt");
        assert!(
            !p.may_prompt() && !p.may_prompt(),
            "retries must not stack prompts"
        );
        p.request();
        assert!(p.may_prompt(), "the user asked for one more");
        assert!(!p.may_prompt());
    }
}
